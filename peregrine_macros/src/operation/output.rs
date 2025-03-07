use crate::operation::{Context, Op};
use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, format_ident, quote};

impl Op {
    pub fn body_function(&self) -> TokenStream {
        let Idents {
            all_reads,
            all_writes,
            write_onlys,
            read_writes,
            op_body_function,
            ..
        } = self.make_idents();

        let body = &self.body;

        quote! {
            fn #op_body_function<'h>(&self, #(#all_reads: <#all_reads as peregrine::resource::Resource<'h>>::Read,)*) -> peregrine::Result<(#(<#all_writes as peregrine::resource::Resource<'h>>::Write,)*)> {
                #(let mut #write_onlys: <#write_onlys as peregrine::resource::Resource<'h>>::Write;)*
                #(let mut #read_writes: <#read_writes as peregrine::resource::Resource<'h>>::Write = #read_writes.into();)*
                #body
                Ok((#(#all_writes,)*))
            }
        }
    }

    fn make_idents(&self) -> Idents {
        let Op {
            context,
            reads,
            writes,
            read_writes,
            uuid,
            ..
        } = self;

        let activity = if let Context::Activity(p) = context {
            p.clone()
        } else {
            todo!()
        };

        let activity_ident = activity.get_ident().unwrap();

        let output = format_ident!("{activity_ident}OpOutput_{uuid}");
        let op = format_ident!("{activity_ident}Op_{uuid}");
        let op_internals = format_ident!("{activity_ident}OpInternals_{uuid}");
        let op_body_function = format_ident!("{activity_ident}_op_body_{uuid}");
        let continuations = format_ident!("{activity_ident}Continuations_{uuid}");
        let downstreams = format_ident!("{activity_ident}Downstreams_{uuid}");

        Idents {
            op_internals,
            op,
            output,
            continuations,
            downstreams,
            op_body_function,
            activity: activity_ident.clone(),
            write_onlys: writes.clone(),
            read_writes: read_writes.clone(),
            all_reads: reads.iter().chain(read_writes.iter()).cloned().collect(),
            all_writes: writes.iter().chain(read_writes.iter()).cloned().collect(),
        }
    }
}

impl ToTokens for Op {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let idents = self.make_idents();
        let definition = generate_operation(&idents);
        let instantiation = result(&idents);

        let result = quote! {
            {
                #definition
                #instantiation
            }
        };

        tokens.extend(result);
    }
}

struct Idents {
    op_internals: Ident,
    op: Ident,
    output: Ident,
    op_body_function: Ident,
    continuations: Ident,
    downstreams: Ident,
    activity: Ident,
    write_onlys: Vec<Ident>,
    read_writes: Vec<Ident>,
    all_reads: Vec<Ident>,
    all_writes: Vec<Ident>,
}

fn generate_operation(idents: &Idents) -> TokenStream {
    let Idents {
        op_internals,
        op,
        output,
        op_body_function,
        continuations,
        downstreams,
        activity,
        all_reads,
        all_writes,
        write_onlys,
        read_writes,
        ..
    } = idents;

    let first_write = &all_writes[0];
    let all_but_one_write = &all_writes[1..];

    let all_read_response_hashes = all_reads
        .iter()
        .map(|i| format_ident!("_peregrine_engine_resource_hash_{i}"))
        .collect::<Vec<_>>();

    let all_read_responses = all_reads
        .iter()
        .map(|i| format_ident!("{i}_response"))
        .collect::<Vec<_>>();

    let read_writes_responses = read_writes
        .iter()
        .map(|i| format_ident!("{i}_response"))
        .collect::<Vec<_>>();

    quote! {
        struct #op_internals<'o, M: peregrine::Model<'o>> {
            grounding_result: Option<peregrine::operation::InternalResult<peregrine::Duration>>,

            #(#all_reads: Option<&'o dyn peregrine::operation::Upstream<'o, #all_reads, M>>,)*
            #(#all_read_responses: Option<peregrine::operation::InternalResult<(u64, <#all_reads as peregrine::resource::Resource<'o>>::Read)>>,)*
        }

        struct #op<'o, M: peregrine::Model<'o> + 'o, G: peregrine::operation::Grounder<'o, M>> {
            grounder: G,

            state: peregrine::reexports::parking_lot::Mutex<peregrine::operation::OperationState<#output<'o>, #continuations<'o, M>, #downstreams<'o, M>>>,

            activity: &'o #activity,
            internals: peregrine::exec::UnsafeSyncCell<#op_internals<'o, M>>
        }

        #[derive(Copy, Clone, Default)]
        struct #output<'h> {
            hash: u64,
            #(#all_writes: <#all_writes as peregrine::resource::Resource<'h>>::Read,)*
        }

        #[allow(non_camel_case_types)]
        enum #continuations<'o, M: peregrine::Model<'o>> {
            #(#all_writes(peregrine::operation::Continuation<'o, #all_writes, M>),)*
        }

        #[allow(non_camel_case_types)]
        enum #downstreams<'o, M: peregrine::Model<'o>> {
            #(#all_writes(peregrine::operation::MaybeMarkedDownstream<'o, #all_writes, M>),)*
        }

        impl<'s, 'o: 's, M: peregrine::Model<'o> + 'o, G: peregrine::operation::Grounder<'o, M>> #op<'o, M, G> {
            fn new(grounder: G, activity: &'o #activity) -> Self {
                #op {
                    state: Default::default(),

                    activity,
                    internals: peregrine::exec::UnsafeSyncCell::new(#op_internals {
                        grounding_result: grounder.get_static().map(|d| Ok(d)),

                        #(#all_reads: None,)*
                        #(#all_read_responses: None,)*
                    }),
                    grounder,
                }
            }
            fn run_continuations(&self, mut state: peregrine::reexports::parking_lot::MutexGuard<peregrine::operation::OperationState<#output<'o>, #continuations<'o, M>, #downstreams<'o, M>>>, scope: &peregrine::reexports::rayon::Scope<'s>, timelines: &'s peregrine::timeline::Timelines<'o, M>, env: peregrine::exec::ExecEnvironment<'s, 'o>) {
                let mut swapped_continuations = peregrine::reexports::smallvec::SmallVec::new();
                std::mem::swap(&mut state.continuations, &mut swapped_continuations);
                let output = state.status.unwrap_done();
                drop(state);

                let start_index = if env.stack_counter < peregrine::exec::STACK_LIMIT { 1 } else { 0 };

                for c in swapped_continuations.drain(start_index..) {
                    match c {
                        #(#continuations::#all_writes(c) => {
                            scope.spawn(move |s| c.run(output.map(|r| (r.hash, r.#all_writes)), s, timelines, env.reset()));
                        })*
                    }
                }

                if env.stack_counter < peregrine::exec::STACK_LIMIT {
                    match swapped_continuations.remove(0) {
                        #(#continuations::#all_writes(c) => {
                            c.run(output.map(|r| (r.hash, r.#all_writes)), scope, timelines, env.increment());
                        })*
                    }
                }
            }

            fn send_requests(&'o self, mut state: peregrine::reexports::parking_lot::MutexGuard<peregrine::operation::OperationState<#output<'o>, #continuations<'o, M>, #downstreams<'o, M>>>, time: peregrine::Duration, scope: &peregrine::reexports::rayon::Scope<'s>, timelines: &'s peregrine::timeline::Timelines<'o, M>, env: peregrine::exec::ExecEnvironment<'s, 'o>) {
                let internals = self.internals.get();
                let (#(#all_read_responses,)*) = unsafe {
                    (#((*internals).#all_read_responses,)*)
                };
                let mut num_requests = 0u8
                    #(+ #all_read_responses.is_none() as u8)*;
                state.response_counter = num_requests;
                drop(state);
                let time = unsafe {
                    (*internals).grounding_result.unwrap().unwrap()
                };
                #(
                    let already_registered = unsafe {
                        if (*internals).#all_reads.is_none() {
                            (*internals).#all_reads = Some(timelines.find_upstream(time)
                                .expect("Could not find an upstream node. Did you insert before the initial conditions?"));
                            false
                        } else {
                            true
                        }
                    };
                    if #all_read_responses.is_none() {
                        num_requests -= 1;
                        let #all_reads = unsafe {
                            (*internals).#all_reads
                        };
                        let continuation = peregrine::operation::Continuation::Node(self);
                        if num_requests == 0 && env.stack_counter < peregrine::exec::STACK_LIMIT {
                            #all_reads.unwrap().request(continuation, already_registered, scope, timelines, env.increment());
                        } else {
                            scope.spawn(move |s| #all_reads.unwrap().request(continuation, already_registered, s, timelines, env.reset()));
                        }
                    }
                )*
            }

            fn run(&'o self, env: peregrine::exec::ExecEnvironment<'s, 'o>) -> peregrine::operation::InternalResult<#output<'o>> {
                use peregrine::Context;
                use peregrine::activity::ActivityLabel;

                let internals = self.internals.get();

                let (#((#all_read_response_hashes, #all_reads),)*) = unsafe {
                    (#((*internals).#all_read_responses.unwrap()?,)*)
                };

                let hash = {
                    use std::hash::{Hasher, BuildHasher, Hash};

                    let mut state = peregrine::history::PeregrineDefaultHashBuilder::default().build_hasher();
                    std::any::TypeId::of::<#output>().hash(&mut state);

                    #(#all_read_response_hashes.hash(&mut state);)*

                    state.finish()
                };

                let result = if let Some(#first_write) = env.history.get::<#first_write>(hash) {
                    #(let #all_but_one_write = env.history.get::<#all_but_one_write>(hash).expect("expected all write outputs from past run to be written to history");)*
                    Ok(#output {
                        hash,
                        #(#all_writes),*
                    })
                } else {
                    use peregrine::{Activity, Context};
                    use peregrine::activity::ActivityLabel;
                    let time = unsafe {
                        (*self.internals.get()).grounding_result.unwrap().unwrap()
                    };
                    self.activity.#op_body_function(#(#all_reads,)*)
                        .with_context(|| {
                            let time = unsafe {
                                (*self.internals.get()).grounding_result.unwrap().unwrap()
                            };
                            format!("occurred in activity {} at {}", #activity::LABEL, time)
                        })
                        .map(|(#(#all_writes,)*)| #output {
                            hash,
                            #(#all_writes: env.history.insert::<#all_writes>(hash, #all_writes),)*
                        })
                };

                result.map_err(|e| {
                    env.errors.push(e);
                    peregrine::operation::ObservedErrorOutput
                })
            }

            fn clear_cached_downstreams(&self) {
                use peregrine::operation::OperationState;

                let mut state = self.state.lock();
                match state.status {
                    peregrine::operation::OperationStatus::Dormant => {},
                    peregrine::operation::OperationStatus::Done(_) => {
                        state.status = peregrine::operation::OperationStatus::Dormant;
                        for downstream in &state.downstreams {
                            match downstream {
                                #(#downstreams::#all_writes(d) => d.clear_cache(),)*
                            }
                        }
                    }
                    _ => unreachable!()
                }
            }
        }

        impl<'o, M: peregrine::Model<'o> + 'o, G: peregrine::operation::Grounder<'o, M>> peregrine::operation::Node<'o, M> for #op<'o, M, G> {
            fn insert_self(&'o self, timelines: &mut peregrine::timeline::Timelines<'o, M>) -> peregrine::Result<()> {
                let notify_time = self.grounder.min();
                #(
                    let previous = self.grounder.insert_me::<#write_onlys>(self, timelines);
                    assert!(previous.len() > 0);
                    for p in previous {
                        p.notify_downstreams(notify_time);
                    }
                )*
                let internals = self.internals.get();
                #(
                    let previous = self.grounder.insert_me::<#read_writes>(self, timelines);

                    if previous.len() == 1 {
                        let upstream = previous[0];
                        upstream.register_downstream_early(self);
                        unsafe {
                            (*internals).#read_writes = Some(upstream);
                            (*internals).#read_writes_responses = None;
                        }
                    }

                    let min = self.grounder.min();
                    for upstream in previous {
                        upstream.notify_downstreams(min);
                    }
                )*
                Ok(())
            }
            fn remove_self(&self, timelines: &mut peregrine::timeline::Timelines<'o, M>) -> peregrine::Result<()> {
                #(
                    let removed = self.grounder.remove_me::<#all_writes>(timelines);
                    if !removed {
                        peregrine::bail!("Removal failed; could not find self at the expected time.")
                    }
                )*

                let mut state = self.state.lock();
                assert!(state.continuations.is_empty());
                for downstream in state.downstreams.drain(..) {
                    match downstream {
                        #(#downstreams::#all_writes(d) => {
                            d.clear_upstream(None);
                        })*
                    }
                }

                Ok(())
            }
        }

        #(
            impl<'o, M: peregrine::Model<'o> + 'o, G: peregrine::operation::Grounder<'o, M>> peregrine::operation::Downstream<'o, #all_reads, M> for #op<'o, M, G> {
                fn respond<'s>(
                    &'o self,
                    value: peregrine::operation::InternalResult<(u64, <#all_reads as peregrine::resource::Resource<'o>>::Read)>,
                    scope: &peregrine::reexports::rayon::Scope<'s>,
                    timelines: &'s peregrine::timeline::Timelines<'o, M>,
                    env: peregrine::exec::ExecEnvironment<'s, 'o>
                ) where 'o: 's {
                    use peregrine::operation::OperationState;
                    use peregrine::activity::ActivityLabel;


                    unsafe {
                        (*self.internals.get()).#all_read_responses = Some(value);
                    }

                    let mut state = self.state.lock();

                    state.response_counter -= 1;

                    if state.response_counter == 0 {
                        drop(state);

                        let result = self.run(env);

                        let mut state = self.state.lock();
                        state.status = peregrine::operation::OperationStatus::Done(result);

                        self.run_continuations(state, scope, timelines, env);
                    }
                }

                fn clear_cache(&self) {
                    unsafe {
                        (*self.internals.get()).#all_read_responses = None;
                    }
                    self.clear_cached_downstreams();
                }

                fn clear_upstream(&self, time_of_change: Option<peregrine::Duration>) -> bool {
                    let internals = self.internals.get();
                    let (clear, retain) = if let Some(time_of_change) = time_of_change {
                        unsafe {
                            match (*internals).grounding_result {
                                Some(Ok(t)) if time_of_change < t => {
                                    (true, false)
                                }
                                Some(Ok(_)) => (false, true),
                                _ => (false, false)
                            }
                        }
                    } else { (true, false) };

                    if clear {
                        unsafe {
                            (*internals).#all_reads = None;
                            (*internals).#all_read_responses = None;
                        }
                        <Self as peregrine::operation::Downstream::<'o, #all_reads, M>>::clear_cache(self);
                    }

                    retain
                }
            }
        )*

        #(
            impl<'o, M: peregrine::Model<'o> + 'o, G: peregrine::operation::Grounder<'o, M>> peregrine::operation::Upstream<'o, #all_writes, M> for #op<'o, M, G> {
                fn request<'s>(
                    &'o self,
                    continuation: peregrine::operation::Continuation<'o, #all_writes, M>,
                    already_registered: bool,
                    scope: &peregrine::reexports::rayon::Scope<'s>,
                    timelines: &'s peregrine::timeline::Timelines<'o, M>,
                    env: peregrine::exec::ExecEnvironment<'s, 'o>
                ) where 'o: 's {
                    use peregrine::operation::OperationStatus;

                    let mut state = self.state.lock();
                    if !already_registered {
                        if let Some(d) = continuation.to_downstream() {
                            state.downstreams.push(#downstreams::#all_writes(d));
                        }
                    }

                    match state.status {
                        OperationStatus::Dormant => {
                            state.continuations.push(#continuations::#all_writes(continuation));
                            state.status = OperationStatus::Working;
                            match self.grounder.get_static() {
                                Some(t) => self.send_requests(state, t, scope, timelines, env),
                                None => unsafe {
                                    match (*self.internals.get()).grounding_result {
                                        Some(Ok(t)) => self.send_requests(state, t, scope, timelines, env),
                                        Some(Err(_)) => {
                                            let mut state = self.state.lock();
                                            state.status = peregrine::operation::OperationStatus::Done(Err(peregrine::operation::ObservedErrorOutput));
                                            self.run_continuations(state, scope, timelines, env);
                                        }
                                        None => self.grounder.request(peregrine::operation::Continuation::Node(self), false, scope, timelines, env.increment())
                                    }
                                }
                            }
                        }
                        OperationStatus::Done(r) => {
                            drop(state);
                            continuation.run(r.map(|o| (o.hash, o.#all_writes)), scope, timelines, env.increment());
                        }
                        OperationStatus::Working => {
                            state.continuations.push(#continuations::#all_writes(continuation));
                        }
                    }
                }

                fn notify_downstreams(&self, time_of_change: peregrine::Duration) {
                    let mut state = self.state.lock();

                    state.downstreams.retain(|downstream| {
                        match downstream {
                            #downstreams::#all_writes(d) => d.clear_upstream(Some(time_of_change)),
                            _ => true
                        }
                    });
                }

                fn register_downstream_early(&self, downstream: &'o dyn peregrine::operation::Downstream<'o, #all_writes, M>) {
                    self.state.lock().downstreams.push(#downstreams::#all_writes(downstream.into()));
                }
            }
        )*

        impl<'o, M: peregrine::Model<'o> + 'o, G: peregrine::operation::Grounder<'o, M>> peregrine::operation::Upstream<'o, peregrine::operation::ungrounded::peregrine_grounding, M> for #op<'o, M, G> {
            fn request<'s>(
                &'o self,
                continuation: peregrine::operation::Continuation<'o, peregrine::operation::ungrounded::peregrine_grounding, M>,
                already_registered: bool,
                scope: &peregrine::reexports::rayon::Scope<'s>,
                timelines: &'s peregrine::timeline::Timelines<'o, M>,
                env: peregrine::exec::ExecEnvironment<'s, 'o>
            ) where 'o: 's {
                self.grounder.request(continuation, already_registered, scope, timelines, env);
            }

            fn notify_downstreams(&self, time_of_change: peregrine::Duration) {
                unreachable!()
            }

            fn register_downstream_early(&self, downstream: &'o dyn peregrine::operation::Downstream<'o, peregrine::operation::ungrounded::peregrine_grounding, M>) {
                unreachable!()
            }
        }

        impl<'o, M: peregrine::Model<'o> + 'o, G: peregrine::operation::Grounder<'o, M>> peregrine::operation::Downstream<'o, peregrine::operation::ungrounded::peregrine_grounding, M> for #op<'o, M, G> {
            fn respond<'s>(
                &'o self,
                value: peregrine::operation::InternalResult<(u64, peregrine::Duration)>,
                scope: &peregrine::reexports::rayon::Scope<'s>,
                timelines: &'s peregrine::timeline::Timelines<'o, M>,
                env: peregrine::exec::ExecEnvironment<'s, 'o>
            ) where 'o: 's {
                use peregrine::operation::OperationState;
                use peregrine::activity::ActivityLabel;

                unsafe {
                    (*self.internals.get()).grounding_result = Some(value.map(|r| r.1));
                }

                let mut state = self.state.lock();

                if value.is_err() {
                    state.status = peregrine::operation::OperationStatus::Done(Err(peregrine::operation::ObservedErrorOutput));
                    self.run_continuations(state, scope, timelines, env);
                } else if matches!(state.status, peregrine::operation::OperationStatus::Working) {
                    self.send_requests(state, value.unwrap().1, scope, timelines, env);
                }
            }

            fn clear_cache(&self) {
                let internals = self.internals.get();
                unsafe {
                    #(
                        (*internals).#all_reads = None;
                        (*internals).#all_read_responses = None;
                    )*
                }

                self.clear_cached_downstreams();
            }
            fn clear_upstream(&self, time_of_change: Option<peregrine::Duration>) -> bool {
                unreachable!()
            }
        }

        #(
            impl<'o, M: peregrine::Model<'o> + 'o, G: peregrine::operation::Grounder<'o, M> + 'o> AsRef<dyn peregrine::operation::Upstream<'o, #all_writes, M> + 'o> for #op<'o, M, G> {
                fn as_ref(&self) -> &(dyn peregrine::operation::Upstream<'o, #all_writes, M> + 'o) {
                    self
                }
            }

            impl<'o, M: peregrine::Model<'o> + 'o, G: peregrine::operation::Grounder<'o, M> + 'o> peregrine::operation::ungrounded::UngroundedUpstream<'o, #all_writes, M> for #op<'o, M, G> {}
        )*
    }
}

fn result(idents: &Idents) -> TokenStream {
    let Idents { op, .. } = idents;

    quote! {
        |grounder, context, bump: peregrine::reexports::bumpalo_herd::Member<'o>| bump.alloc(#op::<'o, M, _>::new(grounder, context))
    }
}
