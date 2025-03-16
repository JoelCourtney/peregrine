use crate::operation::Op;
use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, format_ident, quote};

impl Op {
    fn body_function(&self) -> TokenStream {
        let Idents {
            all_writes,
            write_onlys,
            read_onlys,
            read_writes,
            ..
        } = self.make_idents();

        let body = &self.body;

        quote! {
            peregrine::reexports::serde_closure::Fn!(move |#(#read_onlys: <<#read_onlys as peregrine::resource::Resource>::Data as peregrine::resource::Data>::Sample,)*
            #(mut #read_writes: <#read_writes as peregrine::resource::Resource>::Data,)*|
            -> peregrine::Result<(#(<#all_writes as peregrine::resource::Resource>::Data,)*)> {
                #(#[allow(unused_mut)] let mut #write_onlys: <#write_onlys as peregrine::resource::Resource>::Data;)*
                #body
                Ok((#(#all_writes,)*))
            })
        }
    }

    fn make_idents(&self) -> Idents {
        let Op {
            reads,
            writes,
            read_writes,
            uuid,
            ..
        } = self;

        let output = format_ident!("OpOutput{uuid}");
        let op = format_ident!("Op{uuid}");
        let op_internals = format_ident!("OpInternals{uuid}");
        let continuations = format_ident!("Continuations{uuid}");
        let downstreams = format_ident!("Downstreams{uuid}");

        Idents {
            op_internals,
            op,
            output,
            continuations,
            downstreams,
            write_onlys: writes.clone(),
            read_onlys: reads.clone(),
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
        let instantiation = result(&idents, self.body_function());

        let result = quote! {
            {
                use peregrine::macro_prelude;
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
    continuations: Ident,
    downstreams: Ident,
    read_onlys: Vec<Ident>,
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
        continuations,
        downstreams,
        all_reads,
        all_writes,
        read_onlys,
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

    let num_reads = all_reads.len();

    let body_function_bound = quote! {
        'o + Send + Sync + std::hash::Hash + ::peregrine::reexports::serde_closure::traits::Fn<
            (#(<<#read_onlys as peregrine::resource::Resource>::Data as peregrine::resource::Data<'o>>::Sample,)*
            #(<#read_writes as peregrine::resource::Resource>::Data,)*), Output=peregrine::Result<(#(<#all_writes as peregrine::resource::Resource>::Data,)*)>>
    };

    let resources_generics_decl = quote! {
        #(#read_onlys: peregrine::resource::Resource,)* #(#write_onlys: peregrine::resource::Resource,)* #(#read_writes: peregrine::resource::Resource,)*
    };
    let resources_generics_usage = quote! {
        #(#read_onlys,)* #(#write_onlys,)* #(#read_writes,)*
    };

    let (internals_generics_decl, internals_generics_usage) = if all_reads.is_empty() {
        (quote! {}, quote! {})
    } else {
        (
            quote! { <'o,  #(#all_reads: macro_prelude::Resource,)*> },
            quote! { <'o,  #(#all_reads,)*> },
        )
    };

    quote! {
        struct #op_internals #internals_generics_decl {
            grounding_result: Option<macro_prelude::InternalResult<macro_prelude::Duration>>,

            #(#all_reads: Option<&'o dyn macro_prelude::Upstream<'o, #all_reads>>,)*
            #(#all_read_responses: Option<macro_prelude::InternalResult<(u64, <<#all_reads as macro_prelude::Resource>::Data as macro_prelude::Data<'o>>::Read)>>,)*
        }

        struct #op<'o, B: #body_function_bound, #resources_generics_decl> {
            placement: peregrine::activity::Placement<'o>,

            state: macro_prelude::parking_lot::Mutex<macro_prelude::OperationState<#output<'o, #(#all_writes,)*>, #continuations<'o, #(#all_writes,)*>, #downstreams<'o, #(#all_writes,)*>>>,

            body: B,
            internals: macro_prelude::UnsafeSyncCell<#op_internals #internals_generics_usage>
        }

        #[derive(Copy, Clone)]
        struct #output<'o, #(#all_writes: macro_prelude::Resource,)*> {
            hash: u64,
            #(#all_writes: <<#all_writes as macro_prelude::Resource>::Data as macro_prelude::Data<'o>>::Read,)*
        }

        #[allow(non_camel_case_types)]
        enum #continuations<'o, #(#all_writes: macro_prelude::Resource,)*> {
            #(#all_writes(macro_prelude::Continuation<'o, #all_writes>),)*
        }

        #[allow(non_camel_case_types)]
        enum #downstreams<'o, #(#all_writes: macro_prelude::Resource,)*> {
            #(#all_writes(&'o dyn macro_prelude::Downstream<'o, #all_writes>),)*
        }

        #[allow(clippy::unused_unit)]
        impl<'s, 'o: 's, B: #body_function_bound, #resources_generics_decl> #op<'o, B, #resources_generics_usage> {
            fn new(placement: macro_prelude::Placement<'o>, body: B) -> Self {
                #op {
                    state: Default::default(),

                    body,
                    internals: macro_prelude::UnsafeSyncCell::new(#op_internals {
                        grounding_result: placement.get_static().map(Ok),

                        #(#all_reads: None,)*
                        #(#all_read_responses: None,)*
                    }),
                    placement,
                }
            }
            fn run_continuations(&self, mut state: macro_prelude::parking_lot::MutexGuard<macro_prelude::OperationState<#output<'o, #(#all_writes,)*>, #continuations<'o, #(#all_writes,)*>, #downstreams<'o, #(#all_writes,)*>>>, scope: &macro_prelude::rayon::Scope<'s>, timelines: &'s macro_prelude::Timelines<'o>, env: macro_prelude::ExecEnvironment<'s, 'o>) {
                let mut swapped_continuations = macro_prelude::smallvec::SmallVec::new();
                std::mem::swap(&mut state.continuations, &mut swapped_continuations);
                let output = state.status.unwrap_done();
                drop(state);

                let start_index = if env.stack_counter < macro_prelude::STACK_LIMIT { 1 } else { 0 };

                let time = unsafe {
                    (*self.internals.get()).grounding_result.unwrap()
                };

                for c in swapped_continuations.drain(start_index..) {
                    match c {
                        #(#continuations::#all_writes(c) => {
                            scope.spawn(move |s| c.run(output.map(|r| (r.hash, r.#all_writes)), s, timelines, env.reset()));
                        })*
                    }
                }

                if env.stack_counter < macro_prelude::STACK_LIMIT {
                    match swapped_continuations.remove(0) {
                        #(#continuations::#all_writes(c) => {
                            c.run(output.map(|r| (r.hash, r.#all_writes)), scope, timelines, env.increment());
                        })*
                    }
                }
            }

            fn send_requests(&'o self, mut state: macro_prelude::parking_lot::MutexGuard<macro_prelude::OperationState<#output<'o, #(#all_writes,)*>, #continuations<'o, #(#all_writes,)*>, #downstreams<'o, #(#all_writes,)*>>>, time: macro_prelude::Duration, scope: &macro_prelude::rayon::Scope<'s>, timelines: &'s macro_prelude::Timelines<'o>, env: macro_prelude::ExecEnvironment<'s, 'o>) {
                let internals = self.internals.get();
                let (#(#all_read_responses,)*) = unsafe {
                    (#((*internals).#all_read_responses,)*)
                };
                let mut num_requests = 0
                    #(+ #all_read_responses.is_none() as u8)*;
                state.response_counter = num_requests;
                drop(state);
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
                        let continuation = macro_prelude::Continuation::Node(self);
                        if num_requests == 0 && env.stack_counter < macro_prelude::STACK_LIMIT {
                            #all_reads.unwrap().request(continuation, already_registered, scope, timelines, env.increment());
                        } else {
                            scope.spawn(move |s| #all_reads.unwrap().request(continuation, already_registered, s, timelines, env.reset()));
                        }
                    }
                )*
            }

            fn run(&'o self, env: macro_prelude::ExecEnvironment<'s, 'o>) -> macro_prelude::InternalResult<#output<'o, #(#all_writes,)*>> {
                use macro_prelude::{Data, Context, MaybeHash};

                let internals = self.internals.get();

                let (#((#all_read_response_hashes, #all_reads),)*) = unsafe {
                    (#((*internals).#all_read_responses.unwrap()?,)*)
                };

                let time_as_epoch = peregrine::timeline::duration_to_epoch(
                    unsafe {
                        (*self.internals.get()).grounding_result.unwrap().unwrap()
                    }
                );

                let (#(#read_writes,)*) = (#(<#read_writes as macro_prelude::Resource>::Data::from_read(#read_writes, time_as_epoch),)*);
                let (#(#read_onlys,)*) = (#(<#read_onlys as macro_prelude::Resource>::Data::sample(&#all_reads, time_as_epoch),)*);

                let hash = {
                    use std::hash::{Hasher, BuildHasher, Hash};

                    let mut state = macro_prelude::PeregrineDefaultHashBuilder::default();
                    <Self as macro_prelude::NodeId>::ID.hash(&mut state);

                    self.body.hash(&mut state);

                    #(
                        if #all_reads.is_hashable() {
                            #all_reads.hash_unchecked(&mut state);
                        } else {
                            #all_read_response_hashes.hash(&mut state);
                        }
                    )*

                    state.finish()
                };

                let result = if let Some(#first_write) = env.history.get::<#first_write>(hash, time_as_epoch) {
                    #(let #all_but_one_write = env.history.get::<#all_but_one_write>(hash, time_as_epoch).expect("expected all write outputs from past run to be written to history");)*
                    Ok(#output {
                        hash,
                        #(#all_writes),*
                    })
                } else {
                    self.body.call((#(#read_onlys,)* #(#read_writes,)*))
                        .with_context(|| {
                            format!("occurred at {}", time_as_epoch)
                        })
                        .map(|(#(#all_writes,)*)| #output {
                            hash,
                            #(#all_writes: env.history.insert::<#all_writes>(hash, #all_writes, time_as_epoch),)*
                        })
                };

                result.map_err(|e| {
                    env.errors.push(e);
                    macro_prelude::ObservedErrorOutput
                })
            }

            fn clear_cached_downstreams(&self) {
                let mut state = self.state.lock();
                match state.status {
                    macro_prelude::OperationStatus::Dormant => {},
                    macro_prelude::OperationStatus::Done(_) => {
                        state.status = macro_prelude::OperationStatus::Dormant;
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

        impl<'o, B: #body_function_bound, #resources_generics_decl> macro_prelude::NodeId for #op<'o, B, #resources_generics_usage> {
            const ID: u64 = peregrine::reexports::peregrine_macros::random_u64!();
        }

        impl<'o, B: #body_function_bound, #resources_generics_decl> macro_prelude::Node<'o> for #op<'o, B, #resources_generics_usage> {
            fn insert_self(&'o self, timelines: &mut macro_prelude::Timelines<'o>) -> macro_prelude::Result<()> {
                let notify_time = self.placement.min();
                #(
                    let previous = self.placement.insert_me::<#write_onlys>(self, timelines);
                    assert!(!previous.is_empty());
                    for p in previous {
                        p.notify_downstreams(notify_time);
                    }
                )*
                let internals = self.internals.get();
                #(
                    let previous = self.placement.insert_me::<#read_writes>(self, timelines);

                    if previous.len() == 1 {
                        let upstream = previous[0];
                        upstream.register_downstream_early(self);
                        unsafe {
                            (*internals).#read_writes = Some(upstream);
                            (*internals).#read_writes_responses = None;
                        }
                    }

                    let min = self.placement.min();
                    for upstream in previous {
                        upstream.notify_downstreams(min);
                    }
                )*
                Ok(())
            }
            fn remove_self(&self, timelines: &mut macro_prelude::Timelines<'o>) -> macro_prelude::Result<()> {
                #(
                    let removed = self.placement.remove_me::<#all_writes>(timelines);
                    if !removed {
                        macro_prelude::bail!("Removal failed; could not find self at the expected time.")
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

        impl<'o, B: #body_function_bound, #resources_generics_decl R: macro_prelude::Resource> macro_prelude::Downstream<'o, R> for #op<'o, B, #resources_generics_usage> {
            fn respond<'s>(
                &'o self,
                value: macro_prelude::InternalResult<(u64, <R::Data as macro_prelude::Data<'o>>::Read)>,
                scope: &macro_prelude::rayon::Scope<'s>,
                timelines: &'s macro_prelude::Timelines<'o>,
                env: macro_prelude::ExecEnvironment<'s, 'o>
            ) where 'o: 's {
                macro_prelude::castaway::match_type!(R::INSTANCE, {
                    #(
                        #all_reads as _ => {
                            assert!(
                                std::mem::size_of::<<#all_reads::Data as macro_prelude::Data<'o>>::Read>()
                                    == std::mem::size_of::<<R::Data as macro_prelude::Data<'o>>::Read>()
                            );
                            // Potentially the least safe code ever written.
                            unsafe {
                                let transmuted = std::mem::transmute_copy(&value);
                                std::mem::forget(value);
                                (*self.internals.get()).#all_read_responses = Some(transmuted);
                            }
                        },
                    )*
                    _ => unreachable!()
                });

                let mut state = self.state.lock();

                state.response_counter -= 1;

                if state.response_counter == 0 {
                    drop(state);

                    let result = self.run(env);

                    let mut state = self.state.lock();
                    state.status = macro_prelude::OperationStatus::Done(result);

                    self.run_continuations(state, scope, timelines, env);
                }
            }

            fn clear_cache(&self) {
                macro_prelude::castaway::match_type!(R::INSTANCE, {
                    #(
                        #all_reads as _ => {
                            unsafe {
                                (*self.internals.get()).#all_read_responses = None;
                            }
                        },
                    )*
                    _ => unreachable!()
                });
                self.clear_cached_downstreams();
            }

            fn clear_upstream(&self, time_of_change: Option<macro_prelude::Duration>) -> bool {
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
                    macro_prelude::castaway::match_type!(R::INSTANCE, {
                        #(
                            #all_reads as _ => {
                                unsafe {
                                    (*internals).#all_reads = None;
                                    (*internals).#all_read_responses = None;
                                }
                                <Self as macro_prelude::Downstream::<'o, #all_reads>>::clear_cache(self);
                            },
                        )*
                        _ => unreachable!()
                    });
                }

                retain
            }
        }

        impl<'o, B: #body_function_bound, #resources_generics_decl R: macro_prelude::Resource> macro_prelude::Upstream<'o, R> for #op<'o, B, #resources_generics_usage> {
            fn request<'s>(
                &'o self,
                continuation: macro_prelude::Continuation<'o, R>,
                already_registered: bool,
                scope: &macro_prelude::rayon::Scope<'s>,
                timelines: &'s macro_prelude::Timelines<'o>,
                env: macro_prelude::ExecEnvironment<'s, 'o>
            ) where 'o: 's {
                let mut state = self.state.lock();
                if !already_registered {
                    if let Some(d) = continuation.to_downstream() {
                        macro_prelude::castaway::match_type!(R::INSTANCE, {
                            #(
                                #all_writes as _ => state.downstreams.push(#downstreams::#all_writes(
                                    unsafe { std::mem::transmute(d) }
                                )),
                            )*
                            _ => unreachable!()
                        });
                    }
                }

                match state.status {
                    macro_prelude::OperationStatus::Dormant => {
                        macro_prelude::castaway::match_type!(R::INSTANCE, {
                            #(
                                #all_writes as _ => state.continuations.push(#continuations::#all_writes(
                                    unsafe { std::mem::transmute(continuation) }
                                )),
                            )*
                            _ => unreachable!()
                        });
                        state.status = macro_prelude::OperationStatus::Working;
                        match self.placement.get_static() {
                            Some(t) => {
                                if #num_reads == 0 {
                                    drop(state);
                                    let result = self.run(env);

                                    let mut state = self.state.lock();
                                    state.status = macro_prelude::OperationStatus::Done(result);

                                    self.run_continuations(state, scope, timelines, env);
                                } else {
                                    self.send_requests(state, t, scope, timelines, env);
                                }
                            }
                            None => unsafe {
                                match (*self.internals.get()).grounding_result {
                                    Some(Ok(t)) => self.send_requests(state, t, scope, timelines, env),
                                    Some(Err(_)) => {
                                        let mut state = self.state.lock();
                                        state.status = macro_prelude::OperationStatus::Done(Err(macro_prelude::ObservedErrorOutput));
                                        self.run_continuations(state, scope, timelines, env);
                                    }
                                    None => self.placement.request_grounding(macro_prelude::GroundingContinuation::Node(0, self), false, scope, timelines, env.increment())
                                }
                            }
                        }
                    }
                    macro_prelude::OperationStatus::Done(r) => {
                        drop(state);
                        let send = r.map(|o| {
                            let time = unsafe {
                                (*self.internals.get()).grounding_result.unwrap().unwrap()
                            };
                            let value = macro_prelude::castaway::match_type!(R::INSTANCE, {
                                #(
                                    #all_writes as _ => {
                                        unsafe { std::mem::transmute_copy(&o.#all_writes) }
                                    },
                                )*
                                _ => unreachable!()
                            });
                            (o.hash, value)
                        });
                        continuation.run(send, scope, timelines, env.increment());
                    }
                    macro_prelude::OperationStatus::Working => {
                        macro_prelude::castaway::match_type!(R::INSTANCE, {
                            #(
                                #all_writes as _ => state.continuations.push(#continuations::#all_writes(
                                    unsafe {
                                        std::mem::transmute(continuation)
                                    }
                                )),
                            )*
                            _ => unreachable!()
                        });
                    }
                }
            }

            fn notify_downstreams(&self, time_of_change: macro_prelude::Duration) {
                let mut state = self.state.lock();

                state.downstreams.retain(|downstream| {
                    match downstream {
                        #(
                            #downstreams::#all_writes(d) if macro_prelude::castaway::cast!(R::INSTANCE, #all_writes).is_ok() => d.clear_upstream(Some(time_of_change)),
                        )*
                        _ => true
                    }
                });
            }

            fn register_downstream_early(&self, downstream: &'o dyn macro_prelude::Downstream<'o, R>) {
                let wrapped = macro_prelude::castaway::match_type!(R::INSTANCE, {
                    #(
                        #all_writes as _ => unsafe {
                            #downstreams::#all_writes(std::mem::transmute(downstream))
                        },
                    )*
                    _ => unreachable!()
                });
                self.state.lock().downstreams.push(wrapped);
            }
        }

        impl<'o, B: #body_function_bound, #resources_generics_decl> macro_prelude::GroundingUpstream<'o> for #op<'o, B, #resources_generics_usage> {
            fn request_grounding<'s>(
                &'o self,
                continuation: macro_prelude::GroundingContinuation<'o>,
                already_registered: bool,
                scope: &macro_prelude::rayon::Scope<'s>,
                timelines: &'s macro_prelude::Timelines<'o>,
                env: macro_prelude::ExecEnvironment<'s, 'o>
            ) where 'o: 's {
                self.placement.request_grounding(continuation, already_registered, scope, timelines, env);
            }
        }

        impl<'o, B: #body_function_bound, #resources_generics_decl> macro_prelude::GroundingDownstream<'o> for #op<'o, B, #resources_generics_usage> {
            fn respond_grounding<'s>(
                &'o self,
                value: macro_prelude::InternalResult<(usize, macro_prelude::Duration)>,
                scope: &macro_prelude::rayon::Scope<'s>,
                timelines: &'s macro_prelude::Timelines<'o>,
                env: macro_prelude::ExecEnvironment<'s, 'o>
            ) where 'o: 's {
                unsafe {
                    (*self.internals.get()).grounding_result = Some(value.map(|r| r.1));
                }

                let mut state = self.state.lock();

                match state.status {
                    macro_prelude::OperationStatus::Dormant => {},
                    macro_prelude::OperationStatus::Working => {
                        if let Ok((_, t)) = value {
                            if #num_reads == 0 {
                                drop(state);
                                let result = self.run(env);

                                let mut state = self.state.lock();
                                state.status = macro_prelude::OperationStatus::Done(result);

                                self.run_continuations(state, scope, timelines, env);
                            } else {
                                self.send_requests(state, t, scope, timelines, env);
                            }
                        } else {
                            state.status = macro_prelude::OperationStatus::Done(Err(macro_prelude::ObservedErrorOutput));
                            self.run_continuations(state, scope, timelines, env);
                        }
                    }
                    macro_prelude::OperationStatus::Done(_) => unreachable!()
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
        }

        impl<'o, B: #body_function_bound, #resources_generics_decl R: macro_prelude::Resource> AsRef<dyn macro_prelude::Upstream<'o, R> + 'o> for #op<'o, B, #resources_generics_usage> {
            fn as_ref(&self) -> &(dyn macro_prelude::Upstream<'o, R> + 'o) {
                self
            }
        }

        impl<'o, B: #body_function_bound, #resources_generics_decl R: macro_prelude::Resource> macro_prelude::UngroundedUpstream<'o, R> for #op<'o, B, #resources_generics_usage> {}
    }
}

fn result(idents: &Idents, body_function: TokenStream) -> TokenStream {
    let Idents {
        op,
        read_onlys,
        write_onlys,
        read_writes,
        ..
    } = idents;

    let resources_generics = quote! {
        #(#read_onlys,)* #(#write_onlys,)* #(#read_writes)*
    };

    quote! {
        move |placement| #op::<'_,_, #resources_generics>::new(placement, #body_function)
    }
}
