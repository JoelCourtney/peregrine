use proc_macro2::{Ident, TokenStream};
use quote::quote;

pub struct Node {
    pub name: Ident,
    pub reads_name: Ident,
    pub writes_name: Ident,
    pub continuations_name: Ident,
    pub downstreams_name: Ident,

    pub read_only_upstreams: Vec<Ident>,
    pub read_write_upstreams: Vec<Ident>,
    pub read_only_responses: Vec<Ident>,
    pub read_write_responses: Vec<Ident>,
    pub read_response_hashes: Vec<Ident>,
    pub writes: Vec<Ident>,

    pub read_only_types: Vec<Ident>,
    pub read_write_types: Vec<Ident>,
    pub write_only_types: Vec<Ident>,
}

impl Node {
    pub fn generate(self) -> TokenStream {
        let Node {
            name,
            reads_name,
            writes_name,
            continuations_name,
            downstreams_name,
            read_only_upstreams,
            read_write_upstreams,
            read_only_responses,
            read_write_responses,
            read_response_hashes,
            writes,
            read_only_types,
            read_write_types,
            write_only_types,
        } = self;

        let read_upstreams = read_only_upstreams
            .iter()
            .chain(read_write_upstreams.iter())
            .collect::<Vec<_>>();
        let read_responses = read_only_responses
            .iter()
            .chain(read_write_responses.iter())
            .collect::<Vec<_>>();

        let num_reads = read_upstreams.len();

        let first_write = &writes[0];
        let all_but_one_write = &writes[1..];

        let read_types = read_only_types
            .iter()
            .chain(read_write_types.iter())
            .collect::<Vec<_>>();
        let write_types = write_only_types
            .iter()
            .chain(read_write_types.iter())
            .collect::<Vec<_>>();

        let first_write_type = write_types[0];
        let all_but_one_write_type = &write_types[1..];

        let body_function_bound = quote! {
        'o + Send + Sync + std::hash::Hash + peregrine::reexports::serde_closure::traits::Fn<
                (#(<<#read_only_types as peregrine::resource::Resource>::Data as peregrine::resource::Data<'o>>::Sample,)*
                #(<#read_write_types as peregrine::resource::Resource>::Data,)*), Output=peregrine::Result<(#(<#write_types as peregrine::resource::Resource>::Data,)*)>>
        };

        let resources_generics_decl = quote! {
            #(#read_only_types: peregrine::resource::Resource,)* #(#write_only_types: peregrine::resource::Resource,)* #(#read_write_types: peregrine::resource::Resource,)*
        };
        let resources_generics_usage = quote! {
            #(#read_only_types,)* #(#write_only_types,)* #(#read_write_types,)*
        };

        quote! {
            pub struct #name<'o, B: #body_function_bound, #resources_generics_decl> {
                placement: peregrine::activity::Placement<'o>,

                state: macro_prelude::parking_lot::Mutex<macro_prelude::OperationState<(u64, #writes_name<'o, #(#write_types,)*>), #continuations_name<'o, #(#write_types,)*>, #downstreams_name<'o, #(#write_types,)*>>>,

                body: B,
                reads: macro_prelude::UnsafeSyncCell<#reads_name<'o, #(#read_types,)*>>,
                grounding_result: macro_prelude::UnsafeSyncCell<Option<macro_prelude::InternalResult<macro_prelude::Duration>>>
            }

            #[allow(clippy::unused_unit)]
            impl<'s, 'o: 's, B: #body_function_bound, #resources_generics_decl> #name<'o, B, #resources_generics_usage> {
                pub fn new(placement: macro_prelude::Placement<'o>, body: B) -> Self {
                    #name {
                        state: Default::default(),
                        body,
                        reads: Default::default(),
                        grounding_result: macro_prelude::UnsafeSyncCell::new(placement.get_static().map(Ok)),
                        placement,
                    }
                }
                fn run_continuations(&self, mut state: macro_prelude::parking_lot::MutexGuard<macro_prelude::OperationState<(u64, #writes_name<'o, #(#write_types,)*>), #continuations_name<'o, #(#write_types,)*>, #downstreams_name<'o, #(#write_types,)*>>>, scope: &macro_prelude::rayon::Scope<'s>, timelines: &'s macro_prelude::Timelines<'o>, env: macro_prelude::ExecEnvironment<'s, 'o>) {
                    let mut swapped_continuations = macro_prelude::smallvec::SmallVec::new();
                    std::mem::swap(&mut state.continuations, &mut swapped_continuations);
                    let output = state.status.unwrap_done();
                    drop(state);

                    let start_index = if env.stack_counter < macro_prelude::STACK_LIMIT { 1 } else { 0 };

                    let time = unsafe {
                        (*self.grounding_result.get()).unwrap()
                    };

                    for c in swapped_continuations.drain(start_index..) {
                        match c {
                            #(#continuations_name::#writes(c) => {
                                scope.spawn(move |s| c.run(output.map(|r| (r.0, r.1.#writes)), s, timelines, env.reset()));
                            })*
                        }
                    }

                    if env.stack_counter < macro_prelude::STACK_LIMIT {
                        match swapped_continuations.remove(0) {
                            #(#continuations_name::#writes(c) => {
                                c.run(output.map(|r| (r.0, r.1.#writes)), scope, timelines, env.increment());
                            })*
                        }
                    }
                }

                fn send_requests(&'o self, mut state: macro_prelude::parking_lot::MutexGuard<macro_prelude::OperationState<(u64, #writes_name<'o, #(#write_types,)*>), #continuations_name<'o, #(#write_types,)*>, #downstreams_name<'o, #(#write_types,)*>>>, time: macro_prelude::Duration, scope: &macro_prelude::rayon::Scope<'s>, timelines: &'s macro_prelude::Timelines<'o>, env: macro_prelude::ExecEnvironment<'s, 'o>) {
                    let reads = self.reads.get();
                    let (#(#read_responses,)*) = unsafe {
                        (#((*reads).#read_responses,)*)
                    };
                    let mut num_requests = 0
                        #(+ #read_responses.is_none() as u8)*;
                    state.response_counter = num_requests;
                    drop(state);
                    #(
                        let already_registered = unsafe {
                            if (*reads).#read_upstreams.is_none() {
                                (*reads).#read_upstreams = Some(timelines.find_upstream(time));
                                false
                            } else {
                                true
                            }
                        };
                        if #read_responses.is_none() {
                            num_requests -= 1;
                            let #read_upstreams = unsafe {
                                (*reads).#read_upstreams
                            };
                            let continuation = macro_prelude::Continuation::Node(self);
                            if num_requests == 0 && env.stack_counter < macro_prelude::STACK_LIMIT {
                                #read_upstreams.unwrap().request(continuation, already_registered, scope, timelines, env.increment());
                            } else {
                                scope.spawn(move |s| #read_upstreams.unwrap().request(continuation, already_registered, s, timelines, env.reset()));
                            }
                        }
                    )*
                }

                fn run(&'o self, env: macro_prelude::ExecEnvironment<'s, 'o>) -> macro_prelude::InternalResult<(u64, #writes_name<'o, #(#write_types,)*>)> {
                    use macro_prelude::{Data, Context, MaybeHash};

                    let reads = self.reads.get();

                    let (#((#read_response_hashes, #read_responses),)*) = unsafe {
                        (#((*reads).#read_responses.unwrap()?,)*)
                    };

                    let time_as_epoch = peregrine::timeline::duration_to_epoch(
                        unsafe {
                            (*self.grounding_result.get()).unwrap().unwrap()
                        }
                    );

                    let (#(#read_write_responses,)*) = (#(<#read_write_types as macro_prelude::Resource>::Data::from_read(#read_write_responses, time_as_epoch),)*);
                    let (#(#read_only_responses,)*) = (#(<#read_only_types as macro_prelude::Resource>::Data::sample(&#read_only_responses, time_as_epoch),)*);

                    let hash = {
                        use std::hash::{Hasher, BuildHasher, Hash};

                        let mut state = macro_prelude::PeregrineDefaultHashBuilder::default();

                        self.body.hash(&mut state);

                        #(
                            if #read_responses.is_hashable() {
                                #read_responses.hash_unchecked(&mut state);
                            } else {
                                #read_response_hashes.hash(&mut state);
                            }
                        )*

                        state.finish()
                    };

                    let result = if let Some(#first_write) = env.history.get::<#first_write_type>(hash, time_as_epoch) {
                        #(let #all_but_one_write = env.history.get::<#all_but_one_write_type>(hash, time_as_epoch).expect("expected all write outputs from past run to be written to history");)*
                        Ok((hash, #writes_name {
                            #(#writes),*
                        }))
                    } else {
                        self.body.call((#(#read_only_responses,)* #(#read_write_responses,)*))
                            .with_context(|| {
                                format!("occurred at {}", time_as_epoch)
                            })
                            .map(|(#(#writes,)*)| (hash, #writes_name {
                                #(#writes: env.history.insert::<#write_types>(hash, #writes, time_as_epoch),)*
                            }))
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
                                    #(#downstreams_name::#writes(d) => d.clear_cache(),)*
                                }
                            }
                        }
                        _ => unreachable!()
                    }
                }
            }

            impl<'o, B: #body_function_bound, #resources_generics_decl> macro_prelude::NodeId for #name<'o, B, #resources_generics_usage> {
                const ID: u64 = peregrine::reexports::peregrine_macros::random_u64!();
            }

            impl<'o, B: #body_function_bound, #resources_generics_decl> macro_prelude::Node<'o> for #name<'o, B, #resources_generics_usage> {
                fn insert_self(&'o self, timelines: &macro_prelude::Timelines<'o>) -> macro_prelude::Result<()> {
                    let notify_time = self.placement.min();
                    #(
                        let previous = self.placement.insert_me::<#write_types>(self, timelines);
                        assert!(!previous.is_empty());
                        for p in previous {
                            p.notify_downstreams(notify_time);
                        }
                    )*
                    Ok(())
                }
                fn remove_self(&self, timelines: &macro_prelude::Timelines<'o>) -> macro_prelude::Result<()> {
                    #(
                        let removed = self.placement.remove_me::<#write_types>(timelines);
                        if !removed {
                            macro_prelude::bail!("Removal failed; could not find self at the expected time.")
                        }
                    )*

                    let mut state = self.state.lock();
                    assert!(state.continuations.is_empty());
                    for downstream in state.downstreams.drain(..) {
                        match downstream {
                            #(#downstreams_name::#writes(d) => {
                                d.clear_upstream(None);
                            })*
                        }
                    }

                    Ok(())
                }
            }

            #[allow(unreachable_code)]
            impl<'o, B: #body_function_bound, #resources_generics_decl R: macro_prelude::Resource> macro_prelude::Downstream<'o, R> for #name<'o, B, #resources_generics_usage> {
                fn respond<'s>(
                    &'o self,
                    value: macro_prelude::InternalResult<(u64, <R::Data as macro_prelude::Data<'o>>::Read)>,
                    scope: &macro_prelude::rayon::Scope<'s>,
                    timelines: &'s macro_prelude::Timelines<'o>,
                    env: macro_prelude::ExecEnvironment<'s, 'o>
                ) where 'o: 's {
                    macro_prelude::castaway::match_type!(R::INSTANCE, {
                        #(
                            #read_types as _ => {
                                assert!(
                                    std::mem::size_of::<<#read_types::Data as macro_prelude::Data<'o>>::Read>()
                                        == std::mem::size_of::<<R::Data as macro_prelude::Data<'o>>::Read>()
                                );
                                // Potentially the least safe code ever written.
                                unsafe {
                                    let transmuted = std::mem::transmute_copy(&value);
                                    std::mem::forget(value);
                                    (*self.reads.get()).#read_responses = Some(transmuted);
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
                            #read_types as _ => {
                                unsafe {
                                    (*self.reads.get()).#read_responses = None;
                                }
                            },
                        )*
                        _ => unreachable!()
                    });
                    self.clear_cached_downstreams();
                }

                fn clear_upstream(&self, time_of_change: Option<macro_prelude::Duration>) -> bool {
                    let (clear, retain) = if let Some(time_of_change) = time_of_change {
                        unsafe {
                            match *self.grounding_result.get() {
                                Some(Ok(t)) if time_of_change < t => {
                                    (true, false)
                                }
                                Some(Ok(_)) => (false, true),
                                _ => (false, false)
                            }
                        }
                    } else { (true, false) };

                    if clear {
                        let reads = self.reads.get();
                        macro_prelude::castaway::match_type!(R::INSTANCE, {
                            #(
                                #read_types as _ => {
                                    unsafe {
                                        (*reads).#read_upstreams = None;
                                        (*reads).#read_responses = None;
                                    }
                                    <Self as macro_prelude::Downstream::<'o, #read_types>>::clear_cache(self);
                                },
                            )*
                            _ => unreachable!()
                        });
                    }

                    retain
                }
            }

            impl<'o, B: #body_function_bound, #resources_generics_decl> macro_prelude::GroundingDownstream<'o> for #name<'o, B, #resources_generics_usage> {
                fn respond_grounding<'s>(
                    &'o self,
                    value: macro_prelude::InternalResult<(usize, macro_prelude::Duration)>,
                    scope: &macro_prelude::rayon::Scope<'s>,
                    timelines: &'s macro_prelude::Timelines<'o>,
                    env: macro_prelude::ExecEnvironment<'s, 'o>
                ) where 'o: 's {
                    unsafe {
                        (*self.grounding_result.get()) = Some(value.map(|r| r.1));
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

                fn clear_grounding_cache(&self) {
                    let reads = self.reads.get();
                    unsafe {
                        #(
                            (*reads).#read_upstreams = None;
                            (*reads).#read_responses = None;
                        )*
                    }

                    self.clear_cached_downstreams();
                }
            }

            impl<'o, B: #body_function_bound, #resources_generics_decl R: macro_prelude::Resource> macro_prelude::Upstream<'o, R> for #name<'o, B, #resources_generics_usage> {
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
                                    #write_types as _ => state.downstreams.push(#downstreams_name::#writes(
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
                                    #write_types as _ => state.continuations.push(#continuations_name::#writes(
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
                                    match *self.grounding_result.get() {
                                        Some(Ok(t)) => self.send_requests(state, t, scope, timelines, env),
                                        Some(Err(_)) => {
                                            state.status = macro_prelude::OperationStatus::Done(Err(macro_prelude::ObservedErrorOutput));
                                            self.run_continuations(state, scope, timelines, env);
                                        }
                                        None => {
                                            drop(state);
                                            self.placement.request_grounding(macro_prelude::GroundingContinuation::Node(0, self), false, scope, timelines, env.increment())
                                        }
                                    }
                                }
                            }
                        }
                        macro_prelude::OperationStatus::Done(r) => {
                            drop(state);
                            let send = r.map(|o| {
                                let time = unsafe {
                                    (*self.grounding_result.get()).unwrap().unwrap()
                                };
                                let value = macro_prelude::castaway::match_type!(R::INSTANCE, {
                                    #(
                                        #write_types as _ => {
                                            unsafe { std::mem::transmute_copy(&o.1.#writes) }
                                        },
                                    )*
                                    _ => unreachable!()
                                });
                                (o.0, value)
                            });
                            continuation.run(send, scope, timelines, env.increment());
                        }
                        macro_prelude::OperationStatus::Working => {
                            macro_prelude::castaway::match_type!(R::INSTANCE, {
                                #(
                                    #write_types as _ => state.continuations.push(#continuations_name::#writes(
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
                                #downstreams_name::#writes(d) if macro_prelude::castaway::cast!(R::INSTANCE, #write_types).is_ok() => d.clear_upstream(Some(time_of_change)),
                            )*
                            _ => true
                        }
                    });
                }

                fn register_downstream_early(&self, downstream: &'o dyn macro_prelude::Downstream<'o, R>) {
                    let wrapped = macro_prelude::castaway::match_type!(R::INSTANCE, {
                        #(
                            #write_types as _ => unsafe {
                                #downstreams_name::#writes(std::mem::transmute(downstream))
                            },
                        )*
                        _ => unreachable!()
                    });
                    self.state.lock().downstreams.push(wrapped);
                }

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
        }
    }
}
