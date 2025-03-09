//! # Peregrine Engine
//!
//! A discrete event spacecraft simulation engine designed for schedulers.
//!
//! Peregrine always does the minimal amount of computation to respond to changes in the plan, and to
//! calculate only the requested resources *at the requested times*. If you only care about a couple
//! resources in the vicinity of a small plan change, then that's all the engine simulates.
//!
//! Peregrine also stores a permanent history of resource states, meaning that simulation is not just
//! incremental with respect to the most recent plan state; it is incremental with respect to all recorded
//! history. If you undo five recent simulated changes and add one new activity, the engine will only
//! simulate change of adding the activity, not of adding one and deleting five.
//!
//! Peregrine performs all simulation with as much parallelism as is mathematically allowed by the
//! configuration of the plan. Even on linear plan structures with no available concurrency, initial (extremely informal) benchmarking
//! suggests that Peregrine's engine overhead is significantly lower than Aerie-Merlin's - simulating
//! millions of simple operations per second instead of thousands. Highly expensive operations may
//! amortize the overhead differences, but will not amortize the parallelism.
//!
//! ## Concepts
//!
//! The engine simulates the evolution of a set of Resources over time, operated on by a
//! set of instantaneous Operations, which themselves are grouped together into Activities.
//!
//! ### Resources & Activities
//!
//! Resources are just variables whose evolution over time is tracked and recorded. Activities
//! contain operations that mutate those resources, and place those operations at specific
//! pre-determined times throughout a plan. This is the only fundamental difference between Peregrine and
//! Merlin; activities declare their operations - when they happen, what resources they read/write -
//! and their total duration ahead of time, before simulation.
//!
//! ### Operations, Dependencies, & Parallelism
//!
//! Operations are the instantaneous discrete events that the engine simulates. The can read and write
//! resources, access activity arguments, and be configured by the activity (ahead of time only).
//! By forcing you to declare which resources you read and write, the engine is able to build a
//! directed acyclic graph of operation dependencies. This DAG enables most of the parallelism and minimal
//! computation I bragged about in the intro. When you make a change and request a view of a resource,
//! the simulation propagates backward through the DAG from the requested range, and evaluation
//! of branches in the graph immediately stop when they find cached values from previous runs.
//!
//! ### History & Incremental Simulation
//!
//! Peregrine records the history of all operations that have been simulated. Currently, this is only
//! recorded per-session, but a persistent system could be implemented in the future. This enables
//! the engine to immediately stop as soon as it encounters a state that it has been in before. Importantly,
//! it recognizes the state using only the structure of the DAG and the initial conditions, not the
//! resource state at the time the operation was previously run. It does this by inductively calculating
//! hashes for each operation: each operation hashes together its own unique ID and the hashes of its dependencies,
//! and only the initial condition operations hash the input. This allows the engine to recognize past
//! states without performing any simulation.
//!
//! Importantly, Peregrine stores history independent of the plan, meaning that it can be shared between
//! branched versions of the same plan, even as they are updated and simulated live, in parallel.
//! For an extremely simplified example, consider a plan working on two mostly-independent subsystems,
//! `A` and `B`. We start with an unsimulated base plan, then branch into two copies for the `A` and
//! `B` teams to work on. Say team `A` simulates their portion of the base plan first. `B`'s work is
//! only *mostly* independent, with some coupling through common resources. Most of the time, `B` doesn't
//! need `A`'s resources, but if they do, `A` has already simulated the base plan and those results can
//! be reused even though they are on a different branch. Then, when the branches are merged, a majority
//! of the final plan has already been simulated. Only the areas that coupled `A` and `B` together need
//! to be resimulated.
//!
//! This approach's main drawback is memory usage. By indiscriminately storing all sim results without
//! knowing if they will ever be reused, it can build up gigabytes of store after simulating on the
//! order of tens of millions of operations. Since the keys in the storage are meaningless hashes,
//! there is currently no good way to prune the history to reduce memory usage. This poses some technical
//! problems for long-running venues, though I don't believe they are insurmountable.
//!
//! ### Models
//!
//! For those familiar with Aerie-Merlin, you might notice that I didn't use the word "Model"
//! in the above descriptions. This is because while in Merlin, the model is a container that creates,
//! specifies, and owns its resources and activities, in Peregrine the model is just a selection of
//! pre-existing resources. Activities are applicable to any model that selects the resources it
//! operates on. This reinterpretation gives a couple advantages:
//! - Easier modularity for levels-of-fidelity. If two models are nearly the same, except one uses
//!   higher fidelity simulation for one subsystem, all the activities that *don't* touch that subsystem
//!   are trivially applicable to both models.
//! - Shared history between models that share resources. History is recorded by resource, not
//!   by model or plan. If the same sub-graph appears in different plans on different models, the history
//!   can still be reused.
//!
//!
//! ## Modelling quick-start
//!
//! First, you need to declare resources to operate on. For that, use the [resource] macro.
//!
//! ```
//! # fn main() {}
//! # use peregrine::resource;
//! resource!(sol_counter: u32);
//! resource!(ref downlink_buffer: Vec<String>);
//! ```
//!
//! See the [resource] macro for more details on how to call it.
//! Next, we can make an activity that logs the current sol to the buffer:
//!
//! ```
//! # fn main() {}
//! # use serde::{Serialize, Deserialize};
//! # use peregrine::{resource, impl_activity, Duration};
//! # resource!(sol_counter: u32);
//! # resource!(ref downlink_buffer: Vec<String>);
//! #[derive(Hash, Serialize, Deserialize)]
//! struct LogCurrentSol {
//!     /// Verbosity is taken in as an activity argument.
//!     verbose: bool,
//! }
//!
//! impl_activity! { for LogCurrentSol
//!     // This is syntactic sugar to declare an operation.
//!     // It occurs at time `start`, reads both `sol_counter` and `downlink_buffer`,
//!     // and writes to `downlink_buffer`.
//!     @(start) {
//!         if self.verbose {
//!             ref mut: downlink_buffer.push(format!("It is currently Sol {}", ref:sol_counter));
//!         } else {
//!             ref mut: downlink_buffer.push(format!("Sol {}", ref:sol_counter));
//!         }
//!     }
//!     Duration::ZERO // Return statement indicates the activity had zero duration
//! }
//! ```
//!
//! Lastly you need to make a model that uses these resources:
//!
//! ```
//! # fn main() {}
//! # use peregrine::{resource, model};
//! # resource!(sol_counter: u32);
//! # resource!(ref downlink_buffer: Vec<String>);
//! model! {
//!     DemoModel(sol_counter, downlink_buffer)
//! }
//! ```
//!
//! This implements the [Model] trait, and generates structs to store initial conditions and plan contents.
//!
//! ## Interaction
//!
//! TODO
//!
//! ## Timekeeping
//!
//! Peregrine uses [hifitime](https://docs.rs/hifitime/latest/hifitime/) for timekeeping. The [Epoch][Time]
//! type, renamed in Peregrine to [Time] for simplicity, is used to order operations and activities.
//! The [Duration] type represents difference between [Time]s. As for why I chose hifitime, this line
//! from their documentation should explain it:
//!
//! > This library is validated against NASA/NAIF SPICE for the Ephemeris Time to Universal
//! > Coordinated Time computations: there are exactly zero nanoseconds of difference between
//! > SPICE and hifitime for the computation of ET and UTC after 01 January 1972.
//!
//! There is a significant performance penalty with this library when constructing large plans, due to
//! its non-trivial comparison and ordering. I believe its worth it for compatibility with SPICE,
//! and the penalty isn't present during simulation anyway.
//!
//! ## Possible Features
//!
//! This project is currently a proof-of-concept, but I've set it up with future development in mind.
//! These features could be implemented if there was demand:
//! - **Stateful activities;** activities that store an internal state as a transient resource that they
//!   bring to the model
//! - **Daemon tasks;** background tasks associated with the model that can either generate a statically-known
//!   set of recurring operations, or create "responsive" operations that are placed immediately after
//!   any other operation writes to a given resource.
//! - **Maybe-reads and maybe-writes;** optimizations for operations that may or may not read or write a
//!   resource.
//! - **Global persistent history;** I made a lot of grand claims about sharing history between plans and
//!   models, but I haven't actually implemented that yet. Storing history on the filesystem is possible
//!   already though.
//! - **Stable graph hashing;** currently there are no guarantees that operations will generate the
//!   same hashes when the program is recompiled, but this could be fixed.
//! - **Linked lists in history;** the above example of accumulating a `Vec<String>` buffer in a resource
//!   is *extremely* inefficient. For every operation that writes to it, the vector will be cloned,
//!   leading to quadratic runtime and memory usage. It is possible but non-trivial to make a linked
//!   list that lives inside the history hashmap and persists through serialization. (In reality it
//!   would be an n-ary tree that branches according to changes in the plan, but for any given simulation
//!   it would appear to be a linked list.)
//! - **Look-back reads;** currently operations can only read the current value of resources when
//!   they happen, but there's no reason why they shouldn't be able to look back to a pre-determined
//!   time.
//! - **Activity anchoring;** activities could be defined relative to other activities, as long as the
//!   relationship is known ahead-of-time.
//! - **Activity spawning;** the activity body could automatically spawn child activities when inserted
//!   into the plan, as long as this spawning is only a function of the activity arguments.
//! - **Probabilistic Caching;** if the overhead of reading/writing history is a problem, I could
//!   potentially do pseudo-random caching (such as "only cache if `hash % 10 == 0`") without a large penalty
//!   to cache misses.
//!
//! ## Impossible Features
//!
//! Peregrine has to impose some restrictions on your activities and operations, so some things are
//! impossible:
//! - **Hidden state;** all state in the simulation must be recorded by the history. Getting around
//!   this restriction is UB.
//! - **Non-reentrant or non-deterministic activities;** the engine assumes that for the same input,
//!   all operations will produce the same output, and if a cached value exists in history then it is valid.
//!   It also assumes that it is OK to only resimulate a portion of an activity's operations.

#![cfg_attr(feature = "nightly", feature(btree_cursors))]

pub mod activity;
pub mod exec;
pub mod history;
pub mod macro_prelude;
pub mod operation;
pub mod reexports;
pub mod resource;
pub mod timeline;

pub use crate::activity::{Activity, ActivityId};
use crate::activity::{DecomposedActivity, Placement};
use crate::exec::{ErrorAccumulator, ExecEnvironment};
pub use crate::history::History;
use crate::operation::InternalResult;
use crate::operation::grounding::peregrine_grounding;
pub use crate::operation::initial_conditions::InitialConditions;
use crate::timeline::{MaybeGrounded, Timelines, duration_to_epoch, epoch_to_duration};
pub use anyhow::{Context, Error, Result, anyhow, bail};
use bumpalo_herd::Herd;
pub use hifitime::{Duration, Epoch as Time};
use oneshot::Receiver;
use operation::Continuation;
use parking_lot::RwLock;
use resource::Resource;

use std::collections::HashMap;
use std::ops::RangeBounds;

pub use peregrine_macros::resource;

/// Creates a model and associated structs from a selection of resources.
///
/// Expects a struct-like item, but without the `struct` keyword. For example:
///
/// ```
/// # fn main() {}
/// # use peregrine::{resource, model};
/// # resource!(res_a: u32);
/// # resource!(res_b: u32);
/// model! {
///     MyModel (
///         res_a,
///         res_b
///     )
/// }
/// ```
///
/// This generates a few types: a vacant `MyModel` type that implements `Model`, as well as
/// structs called `MyModelHistories` and `MyModelInitialConditions`. The initial conditions
/// are used to create a new plan, and has one field for each resource where you can populate
/// the resource's `Write` value. The histories are used to cache simulation results to be reused
/// in later simulations.
pub use peregrine_macros::model;

/// Implements the [Activity] trait for a type.
///
/// Expects a block of statements preceded by `for MyActivity`. The inside of the block is a function
/// that generates the activity's operations, and returns the duration of the activity. The start time
/// is accessible through the `start` variable, and the activity arguments are accessible through `args`.
///
/// The body of your activity function will contain operations that use a special syntactic sugar.
/// Let's break down this example:
///
/// ```
/// # fn main() {}
/// # use peregrine::{resource, impl_activity, Duration};
/// use serde::{Serialize, Deserialize};
///
/// resource!(sol_counter: u32);
///
/// #[derive(Hash, Serialize, Deserialize)]
/// struct IncrementSol;
///
/// impl_activity! { for IncrementSol
///     @(start) {
///         ref mut: sol_counter += 1;
///     }
///     Duration::ZERO // Return statement indicates the activity had zero duration
/// }
/// ```
///
/// 1. First declare an empty struct `IncrementSol` to be our activity type. It has to
///    implement [Serialize], and [DeserializeOwned], and this is done through derive macros
///    provided by serde.
/// 2. Call [impl_activity] with the preamble `for IncrementSol`. Everything else inside the
///    macro is your function body. In this context, `start` is the start time of the activity,
///    and `args` are the arguments (in this case there are none).
/// 3. Declare operation by starting a statement with `@`.
///    - `(start)` indicates the time the operation happens at. It can be any valid rust expression
///      that evaluates to a [Duration].
///    - TODO explain ref mut
///    - The body of the operation can do whatever you want, as long as it is deterministic.
///      The body is also an async context; you could make a non-blocking web request if you want,
///      as long as it can be assumed to always return the same output for the same input.
/// 4. Finally, we end the activity body by returning `Duration::ZERO`, which means the activity took
///    zero duration.
///
/// It is *technically* valid to generate operations before the start time or after the declared end time.
/// It would just be very un-hygienic and potentially hard to debug.
pub use peregrine_macros::impl_activity;

#[derive(Default)]
pub struct Session {
    herd: Herd,
    history: RwLock<History>,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn into_history(self) -> History {
        self.history.into_inner()
    }

    pub fn new_plan<'o, M: Model<'o>>(
        &'o self,
        time: Time,
        initial_conditions: InitialConditions,
    ) -> Plan<'o, M>
    where
        Self: 'o,
    {
        let mut history = self.history.write();
        M::init_history(&mut history);
        drop(history);
        Plan::new(self, time, initial_conditions)
    }
}

impl From<History> for Session {
    fn from(history: History) -> Self {
        Self {
            history: RwLock::new(history),
            ..Self::default()
        }
    }
}

/// A plan session for iterative editing and simulating.
pub struct Plan<'o, M: Model<'o>> {
    activities: HashMap<ActivityId, DecomposedActivity<'o, M>>,
    id_counter: u32,
    timelines: Timelines<'o, M>,

    session: &'o Session,
}

impl<'o, M: Model<'o> + 'o> Plan<'o, M> {
    /// Create a new empty plan from initial conditions and a session.
    fn new(session: &'o Session, time: Time, mut initial_conditions: InitialConditions) -> Self {
        let mut timelines = Timelines::new(&session.herd);
        M::init_timelines::<M>(
            epoch_to_duration(time),
            &mut initial_conditions,
            &mut timelines,
        );
        Plan {
            activities: HashMap::new(),
            timelines,
            id_counter: 0,

            session,
        }
    }

    pub fn reserve_activity_capacity(&mut self, additional: usize) {
        self.activities.reserve(additional);
    }

    /// Inserts a new activity into the plan, and returns its unique ID.
    pub fn insert(
        &mut self,
        time: Time,
        activity: impl Activity<'o, M> + 'static,
    ) -> Result<ActivityId> {
        let id = ActivityId::new(self.id_counter);
        self.id_counter += 1;
        let bump = self.session.herd.get();
        let activity = bump.alloc(activity);
        let activity_pointer = activity as *mut dyn Activity<'o, M>;
        let (_duration, operations) =
            activity.decompose(Placement::Static(epoch_to_duration(time)), bump)?;

        for op in &operations {
            op.insert_self(&mut self.timelines)?;
        }

        self.activities.insert(
            id,
            DecomposedActivity {
                activity: activity_pointer,
                operations,
            },
        );

        Ok(id)
    }

    /// Removes an activity from the plan, by ID.
    pub fn remove(&mut self, id: ActivityId) -> Result<()> {
        let decomposed = self
            .activities
            .remove(&id)
            .ok_or_else(|| anyhow!("could not find activity with id {id:?}"))?;
        for op in decomposed.operations {
            op.remove_self(&mut self.timelines)?;
        }
        unsafe { std::ptr::drop_in_place(decomposed.activity) };

        Ok(())
    }

    /// Returns a view into a section of a resource's timeline. After creating a plan, call
    /// `plan.view::<MyResource>(start..end, &histories)` to get a vector of times and values
    /// within the `start - end` range.
    ///
    /// Try to limit the requested range to only the times that you need.
    ///
    /// The histories struct will be autogenerated by the [model] macro.
    pub fn view<R: Resource<'o> + 'o>(
        &self,
        bounds: impl RangeBounds<Time>,
    ) -> Result<Vec<(Time, R::Read)>>
    where
        Self: 'o,
    {
        let mut nodes: Vec<MaybeGrounded<'o, R, M>> = self.timelines.range((
            bounds.start_bound().map(|t| epoch_to_duration(*t)),
            bounds.end_bound().map(|t| epoch_to_duration(*t)),
        ));

        let mut receivers: Vec<MaybeGroundedResult<'o, R>> = Vec::with_capacity(nodes.len());
        let errors = ErrorAccumulator::default();

        enum MaybeGroundedResult<'o, R: Resource<'o>> {
            Grounded(Duration, Receiver<InternalResult<R::Read>>),
            Ungrounded(
                Receiver<InternalResult<Duration>>,
                Receiver<InternalResult<R::Read>>,
            ),
        }

        let timelines = &self.timelines;

        let history_lock = self.session.history.read();
        let history = unsafe { &*(&*history_lock as *const History).cast::<History>() };

        rayon::scope(|scope| {
            let env = ExecEnvironment {
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
                            n.request(
                                Continuation::<peregrine_grounding, M>::Root(grounding_sender),
                                true,
                                s,
                                timelines,
                                env.reset(),
                            );
                            n.request(
                                Continuation::<R, M>::Root(sender),
                                true,
                                s,
                                timelines,
                                env.reset(),
                            );
                        });
                    }
                }
            }
        });

        if !errors.is_empty() {
            Err(errors.into())
        } else {
            receivers
                .into_iter()
                .map(|r| match r {
                    MaybeGroundedResult::Grounded(t, recv) => {
                        Ok((duration_to_epoch(t), recv.recv().unwrap()?))
                    }
                    MaybeGroundedResult::Ungrounded(t_recv, recv) => Ok((
                        duration_to_epoch(t_recv.recv().unwrap()?),
                        recv.recv().unwrap()?,
                    )),
                })
                .collect()
        }
    }

    pub fn sample<R: Resource<'o> + 'o>(&self, time: Time) -> Result<R::Sample> {
        let (written_at, read) = self
            .view::<R>(time..=time)?
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("No operations to sample found at or before {time}"))?;
        let read_wrapper = R::convert_for_reading(
            R::wrap(read, epoch_to_duration(written_at)),
            epoch_to_duration(time),
        );
        Ok(R::sample(&read_wrapper))
    }
}

impl<'o, M: Model<'o>> Drop for Plan<'o, M> {
    fn drop(&mut self) {
        for decomposed in self.activities.values_mut() {
            unsafe {
                decomposed.activity.drop_in_place();
            }
        }
    }
}

/// A selection of resources, with tools for creating a plan and storing history.
///
/// Autogenerated by the [model] macro.
pub trait Model<'o>: Sync {
    fn init_history(history: &mut History);
    fn init_timelines<M: Model<'o>>(
        time: Duration,
        initial_conditions: &mut InitialConditions,
        timelines: &mut Timelines<'o, M>,
    );
}
