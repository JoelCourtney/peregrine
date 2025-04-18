use crate::exec::ExecEnvironment;
use crate::operation::{
    Continuation, Downstream, InternalResult, Node, ObservedErrorOutput, Upstream,
};
use crate::resource::Resource;
use crate::timeline::Timelines;
use hifitime::Duration;
use parking_lot::Mutex;
use rayon::Scope;
use smallvec::SmallVec;

pub trait UngroundedUpstream<'o, R: Resource>:
    AsRef<dyn Upstream<'o, R> + 'o> + Upstream<'o, R> + GroundingUpstream<'o>
{
}

pub trait GroundingUpstream<'o>: Sync {
    fn request_grounding<'s>(
        &'o self,
        continuation: GroundingContinuation<'o>,
        already_registered: bool,
        scope: &Scope<'s>,
        timelines: &'s Timelines<'o>,
        env: ExecEnvironment<'s, 'o>,
    ) where
        'o: 's;
}

pub trait GroundingDownstream<'o>: Sync {
    fn respond_grounding<'s>(
        &'o self,
        value: InternalResult<(usize, Duration)>,
        scope: &Scope<'s>,
        timelines: &'s Timelines<'o>,
        env: ExecEnvironment<'s, 'o>,
    ) where
        'o: 's;

    fn clear_cache(&self);
}

pub enum GroundingContinuation<'o> {
    Node(usize, &'o dyn GroundingDownstream<'o>),
    Root(oneshot::Sender<InternalResult<Duration>>),
}

impl<'o> GroundingContinuation<'o> {
    pub fn run<'s>(
        self,
        value: InternalResult<Duration>,
        scope: &Scope<'s>,
        timelines: &'s Timelines<'o>,
        env: ExecEnvironment<'s, 'o>,
    ) where
        'o: 's,
    {
        match self {
            GroundingContinuation::Node(marker, node) => {
                node.respond_grounding(value.map(|value| (marker, value)), scope, timelines, env);
            }
            GroundingContinuation::Root(s) => s.send(value).unwrap(),
        }
    }
}

impl<'o> GroundingUpstream<'o> for Duration {
    fn request_grounding<'s>(
        &'o self,
        continuation: GroundingContinuation<'o>,
        _already_registered: bool,
        scope: &Scope<'s>,
        timelines: &'s Timelines<'o>,
        env: ExecEnvironment<'s, 'o>,
    ) where
        'o: 's,
    {
        continuation.run(Ok(*self), scope, timelines, env);
    }
}

pub struct UngroundedUpstreamResolver<'o, R: Resource> {
    time: Duration,
    grounded_upstream: Option<(Duration, &'o dyn Upstream<'o, R>)>,
    ungrounded_upstreams: SmallVec<&'o dyn UngroundedUpstream<'o, R>, 1>,
    grounding_responses: Mutex<SmallVec<InternalResult<(usize, Duration)>, 1>>,
    continuation: Mutex<Option<Continuation<'o, R>>>,
    downstream: Mutex<Option<&'o dyn Downstream<'o, R>>>,

    #[allow(clippy::type_complexity)]
    cached_decision: Mutex<Option<InternalResult<(Duration, &'o dyn Upstream<'o, R>)>>>,
}

impl<'o, R: Resource> UngroundedUpstreamResolver<'o, R> {
    pub(crate) fn new(
        time: Duration,
        grounded: Option<(Duration, &'o dyn Upstream<'o, R>)>,
        ungrounded: SmallVec<&'o dyn UngroundedUpstream<'o, R>, 1>,
    ) -> Self {
        Self {
            time,
            grounded_upstream: grounded,
            ungrounded_upstreams: ungrounded,
            grounding_responses: Mutex::new(SmallVec::new()),
            continuation: Mutex::new(None),
            downstream: Mutex::new(None),
            cached_decision: Mutex::new(None),
        }
    }
}

impl<'o, R: Resource> Node<'o> for UngroundedUpstreamResolver<'o, R> {
    fn insert_self(&'o self, _timelines: &Timelines<'o>) -> anyhow::Result<()> {
        unreachable!()
    }

    fn remove_self(&self, _timelines: &Timelines<'o>) -> anyhow::Result<()> {
        unreachable!()
    }
}

impl<'o, R: Resource> Upstream<'o, R> for UngroundedUpstreamResolver<'o, R> {
    fn request<'s>(
        &'o self,
        continuation: Continuation<'o, R>,
        already_registered: bool,
        scope: &Scope<'s>,
        timelines: &'s Timelines<'o>,
        env: ExecEnvironment<'s, 'o>,
    ) where
        'o: 's,
    {
        let decision = self.cached_decision.lock();
        if let Some(r) = *decision {
            match r {
                Ok((_, u)) => u.request(continuation, false, scope, timelines, env.increment()),
                Err(_) => {
                    continuation.run(Err(ObservedErrorOutput), scope, timelines, env.increment())
                }
            }
            return;
        }
        drop(decision);

        if !already_registered {
            let mut downstream_lock = self.downstream.lock();
            debug_assert!(downstream_lock.is_none());
            *downstream_lock = continuation.to_downstream();
        }

        if !self.ungrounded_upstreams.is_empty() {
            for (i, ungrounded) in self.ungrounded_upstreams[1..].iter().enumerate() {
                scope.spawn(move |s| {
                    ungrounded.request_grounding(
                        GroundingContinuation::Node(i, self),
                        false,
                        s,
                        timelines,
                        env.reset(),
                    )
                });
            }

            self.ungrounded_upstreams[0].request_grounding(
                GroundingContinuation::Node(0, self),
                false,
                scope,
                timelines,
                env.increment(),
            );
        }
    }

    fn notify_downstreams(&self, time_of_change: Duration) {
        let mut downstream = self.downstream.lock();
        let retain = if let Some(d) = &*downstream {
            d.clear_upstream(Some(time_of_change))
        } else {
            false
        };
        if !retain {
            *downstream = None;
        }
    }

    fn register_downstream_early(&self, downstream: &'o dyn Downstream<'o, R>) {
        *self.downstream.lock() = Some(downstream);
    }
}

impl<'o, R: Resource> GroundingDownstream<'o> for UngroundedUpstreamResolver<'o, R> {
    fn respond_grounding<'s>(
        &self,
        value: InternalResult<(usize, Duration)>,
        scope: &Scope<'s>,
        timelines: &'s Timelines<'o>,
        env: ExecEnvironment<'s, 'o>,
    ) where
        'o: 's,
    {
        let mut responses_lock = self.grounding_responses.lock();
        responses_lock.push(value);

        if responses_lock.len() == self.ungrounded_upstreams.len() {
            let folded_result = responses_lock
                .drain(..)
                .collect::<anyhow::Result<SmallVec<_, 1>, _>>();
            let mut decision = self.cached_decision.lock();
            let continuation = self.continuation.lock().take().unwrap();
            match folded_result {
                Err(_) => {
                    *decision = Some(Err(ObservedErrorOutput));
                    continuation.run(Err(ObservedErrorOutput), scope, timelines, env.increment());
                }
                Ok(vec) => {
                    let earliest_ungrounded = vec
                        .iter()
                        .filter(|gr| gr.1 < self.time)
                        .max_by_key(|gr| gr.1);

                    match (earliest_ungrounded, self.grounded_upstream) {
                        (Some(ug), Some(gr)) => {
                            if gr.0 > ug.1 {
                                *decision = Some(Ok(gr));
                            } else {
                                *decision =
                                    Some(Ok((ug.1, self.ungrounded_upstreams[ug.0].as_ref())));
                            }
                        }
                        (Some(ug), None) => {
                            *decision = Some(Ok((ug.1, self.ungrounded_upstreams[ug.0].as_ref())))
                        }
                        (None, Some(gr)) => *decision = Some(Ok(gr)),
                        _ => unreachable!(),
                    }

                    decision.unwrap().unwrap().1.request(
                        continuation,
                        false,
                        scope,
                        timelines,
                        env.increment(),
                    );
                }
            }
        }
    }

    fn clear_cache(&self) {
        *self.cached_decision.lock() = None;
        if let Some(c) = self.downstream.lock().as_ref() {
            c.clear_cache();
        }
    }
}
