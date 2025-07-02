use crate::internal::exec::ErrorAccumulator;
use crate::internal::history::History;
use crate::internal::macro_prelude::GroundingContinuation;
use crate::internal::operation::initial_conditions::InitialConditions;
use crate::internal::operation::{Continuation, InternalResult};
use crate::internal::placement::{DecomposedActivity, DenseTime, Placement};
use crate::internal::timeline::{MaybeGrounded, Timelines, duration_to_epoch, epoch_to_duration};
use crate::public::resource::init_builtins_timelines;
use crate::{Activity, ActivityId, Data, Model, Ops, Resource, Session, Time};
use anyhow::anyhow;
use oneshot::Receiver;
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::marker::PhantomData;
use std::ops::RangeBounds;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// A plan instance for iterative editing and simulating.
pub struct Plan<'o, M: Model<'o>> {
    activities: HashMap<ActivityId, DecomposedActivity<'o>>,
    id_counter: u32,
    order: Arc<AtomicU64>,
    timelines: Timelines<'o>,

    session: &'o Session,

    model: PhantomData<M>,
}

impl<'o, M: Model<'o> + 'o> Plan<'o, M> {
    /// Create a new empty plan from initial conditions and a session.
    pub(crate) fn new(
        session: &'o Session,
        time: Time,
        mut initial_conditions: InitialConditions,
    ) -> anyhow::Result<Self> {
        let time = epoch_to_duration(time);
        let mut timelines = Timelines::new(&session.herd);
        init_builtins_timelines(time, &mut timelines);
        let order = Arc::new(AtomicU64::new(1));
        M::init_timelines(time, &mut initial_conditions, &mut timelines, order.clone())?;
        Ok(Plan {
            activities: HashMap::new(),
            timelines,
            id_counter: 0,
            order,

            session,

            model: PhantomData,
        })
    }

    /// Reserve memory for a large batch of additional activities.
    ///
    /// Provides a noticeable speedup when loading large plans.
    pub fn reserve_activity_capacity(&mut self, additional: usize) {
        self.activities.reserve(additional);
    }

    /// Inserts a new activity into the plan, and returns its unique ID.
    pub fn insert(
        &mut self,
        time: Time,
        activity: impl Activity + 'static,
    ) -> anyhow::Result<ActivityId> {
        let id = ActivityId::new(self.id_counter);
        self.id_counter += 1;
        let bump = self.session.herd.get();
        let activity = bump.alloc(activity);
        let activity_pointer = activity as *mut dyn Activity;

        let operations = RefCell::new(vec![]);
        let placement = Placement::Static(DenseTime::first_at(epoch_to_duration(time)));
        let ops_consumer = Ops {
            placement,
            bump: &bump,
            operations: &operations,
            order: self.order.clone(),
        };

        let _duration = activity.run(ops_consumer)?;

        for op in &*operations.borrow() {
            op.insert_self(&self.timelines, false)?;
        }

        self.activities.insert(
            id,
            DecomposedActivity {
                activity: activity_pointer,
                operations: operations.into_inner(),
            },
        );

        Ok(id)
    }

    /// Removes an activity from the plan, by ID.
    pub fn remove(&mut self, id: ActivityId) -> anyhow::Result<()> {
        let decomposed = self
            .activities
            .remove(&id)
            .ok_or_else(|| anyhow!("could not find activity with id {id:?}"))?;
        for op in decomposed.operations {
            op.remove_self(&self.timelines, false)?;
        }
        unsafe { std::ptr::drop_in_place(decomposed.activity) };

        Ok(())
    }

    /// Simulates and returns a view into a section of a resource's timeline.
    pub fn view<R: Resource>(
        &self,
        bounds: impl RangeBounds<Time>,
    ) -> anyhow::Result<Vec<(Time, <R::Data as Data<'o>>::Read)>> {
        let mut nodes: Vec<MaybeGrounded<'o, R>> = self.timelines.range((
            bounds
                .start_bound()
                .map(|t| DenseTime::first_at(epoch_to_duration(*t))),
            bounds
                .end_bound()
                .map(|t| DenseTime::last_at(epoch_to_duration(*t))),
        ));

        let mut receivers: Vec<MaybeGroundedResult<R>> = Vec::with_capacity(nodes.len());
        let errors = ErrorAccumulator::default();

        enum MaybeGroundedResult<'h, R: Resource> {
            Grounded(
                DenseTime,
                Receiver<InternalResult<<R::Data as Data<'h>>::Read>>,
            ),
            Ungrounded(
                Receiver<InternalResult<DenseTime>>,
                Receiver<InternalResult<<R::Data as Data<'h>>::Read>>,
            ),
        }

        let timelines = &self.timelines;

        let history_lock = self.session.history.read();
        let history = unsafe { &*(&*history_lock as *const History).cast::<History>() };

        rayon::scope(|scope| {
            let env = crate::internal::exec::ExecEnvironment {
                errors: &errors,
                history,
                stack_counter: 0,
            };
            for node in nodes.drain(..) {
                let (sender, receiver) = oneshot::channel();

                match node {
                    MaybeGrounded::Grounded(t, n) => {
                        receivers.push(MaybeGroundedResult::Grounded(t, receiver));
                        scope.spawn(move |s| {
                            n.request(Continuation::Root(sender), true, s, timelines, env.reset())
                        });
                    }
                    MaybeGrounded::Ungrounded(n) => {
                        let (grounding_sender, grounding_receiver) = oneshot::channel();
                        receivers.push(MaybeGroundedResult::Ungrounded(
                            grounding_receiver,
                            receiver,
                        ));
                        scope.spawn(move |s| {
                            n.request_grounding(
                                GroundingContinuation::Root(grounding_sender),
                                true,
                                s,
                                timelines,
                                env.reset(),
                            )
                        });
                        scope.spawn(move |s| {
                            n.request(Continuation::Root(sender), true, s, timelines, env.reset())
                        });
                    }
                }
            }
        });

        let mut result = Vec::with_capacity(receivers.len());
        for receiver in receivers {
            match receiver {
                MaybeGroundedResult::Grounded(time, receiver) => {
                    if let Ok(read) = receiver.recv()? {
                        result.push((duration_to_epoch(time.when), read))
                    }
                }
                MaybeGroundedResult::Ungrounded(grounding_receiver, receiver) => {
                    if let (Ok(grounding_time), Ok(read)) =
                        (grounding_receiver.recv()?, receiver.recv()?)
                    {
                        result.push((duration_to_epoch(grounding_time.when), read))
                    }
                }
            }
        }

        if !errors.is_empty() {
            return Err(anyhow::anyhow!("{:?}", errors));
        }

        Ok(result)
    }

    /// Samples a resource at a specific time.
    pub fn sample<R: Resource>(&self, time: Time) -> anyhow::Result<<R::Data as Data<'o>>::Sample> {
        let view = self
            .view::<R>(time..=time)?
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        let latest = view
            .range(..=time)
            .next_back()
            .ok_or_else(|| anyhow!("No operations to sample found at or before {time}"))?;
        Ok(R::Data::sample(*latest.1, time))
    }
}

impl<'o, M: Model<'o>> Drop for Plan<'o, M> {
    fn drop(&mut self) {
        for decomposed in self.activities.values() {
            unsafe { std::ptr::drop_in_place(decomposed.activity) };
        }
    }
}

impl<'o, M: Model<'o>> Serialize for Plan<'o, M> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.activities.len()))?;
        for id in self.activities.keys() {
            seq.serialize_element(&id)?;
        }
        seq.end()
    }
}
