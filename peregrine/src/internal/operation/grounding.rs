use crate::internal::exec::ExecEnvironment;
use crate::internal::operation::{
    Continuation, Downstream, GroundingDownstream, InternalResult, ObservedErrorOutput, Upstream,
    UpstreamVec,
};
use crate::internal::timeline::Timelines;
use crate::public::resource::Resource;
use hifitime::Duration;
use parking_lot::Mutex;
use rayon::Scope;
use smallvec::SmallVec;

#[allow(unused_imports)]
use crate as peregrine;
use crate::internal::macro_prelude::DenseTime;

pub struct Delay<U> {
    pub min: Duration,
    pub max: Duration,
    pub node: U,
}

// Delay nodes only emit a plain duration, not a DenseTime.
// The order is added at the boundary between Continuation and GroundingContinuation.
peregrine::resource!(pub peregrine_grounding: Duration;);

pub enum GroundingContinuation<'o> {
    Node(usize, &'o dyn GroundingDownstream<'o>),
    Root(oneshot::Sender<InternalResult<DenseTime>>),
}

impl<'o> GroundingContinuation<'o> {
    pub fn run<'s>(
        self,
        value: InternalResult<DenseTime>,
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

pub struct UngroundedUpstreamResolver<'o, R: Resource> {
    time: DenseTime,
    grounded_upstream: Option<(DenseTime, &'o dyn Upstream<'o, R>)>,
    ungrounded_upstreams: UpstreamVec<'o, R>,
    grounding_responses: Mutex<SmallVec<InternalResult<(usize, DenseTime)>, 1>>,
    continuation: Mutex<Option<Continuation<'o, R>>>,
    downstream: Mutex<Option<&'o dyn Downstream<'o, R>>>,

    #[allow(clippy::type_complexity)]
    cached_decision: Mutex<Option<InternalResult<(DenseTime, &'o dyn Upstream<'o, R>)>>>,
}

impl<'o, R: Resource> UngroundedUpstreamResolver<'o, R> {
    pub(crate) fn new(
        time: DenseTime,
        grounded: Option<(DenseTime, &'o dyn Upstream<'o, R>)>,
        ungrounded: UpstreamVec<'o, R>,
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
                Err(_) => continuation.run(
                    Err(ObservedErrorOutput),
                    0,
                    scope,
                    timelines,
                    env.increment(),
                ),
            }
            return;
        }
        drop(decision);

        if !already_registered {
            let mut downstream_lock = self.downstream.lock();
            debug_assert!(downstream_lock.is_none());
            *downstream_lock = continuation.to_downstream();
        }

        debug_assert!(!self.ungrounded_upstreams.is_empty());

        let mut continuation_lock = self.continuation.lock();
        debug_assert!(continuation_lock.is_none());
        *continuation_lock = Some(continuation);
        drop(continuation_lock);

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

    fn notify_downstreams(&self, time_of_change: DenseTime) {
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

    fn request_grounding<'s>(
        &'o self,
        _continuation: GroundingContinuation<'o>,
        _already_registered: bool,
        _scope: &Scope<'s>,
        _timelines: &'s Timelines<'o>,
        _env: ExecEnvironment<'s, 'o>,
    ) where
        'o: 's,
    {
        unreachable!()
    }
}

impl<'o, R: Resource> GroundingDownstream<'o> for UngroundedUpstreamResolver<'o, R> {
    fn respond_grounding<'s>(
        &self,
        value: InternalResult<(usize, DenseTime)>,
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
                    continuation.run(
                        Err(ObservedErrorOutput),
                        0,
                        scope,
                        timelines,
                        env.increment(),
                    );
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
                                *decision = Some(Ok((ug.1, self.ungrounded_upstreams[ug.0])));
                            }
                        }
                        (Some(ug), None) => {
                            *decision = Some(Ok((ug.1, self.ungrounded_upstreams[ug.0])))
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

    fn clear_grounding_cache(&self) {
        *self.cached_decision.lock() = None;
        if let Some(c) = self.downstream.lock().as_ref() {
            c.clear_cache();
        }
    }
}
