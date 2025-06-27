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
        'o + Send + Sync + std::hash::Hash + serde_closure::traits::Fn<
                (#(<<#read_only_types as Resource>::Data as Data<'o>>::Sample,)*
                #(<#read_write_types as Resource>::Data,)*), Output=Result<(#(<#write_types as Resource>::Data,)*)>>
        };

        let resources_generics_decl = quote! {
            #(#read_only_types: Resource,)* #(#write_only_types: Resource,)* #(#read_write_types: Resource,)*
        };
        let resources_generics_usage = quote! {
            #(#read_only_types,)* #(#write_only_types,)* #(#read_write_types,)*
        };

        quote! {
            pub struct #name<'o, B: #body_function_bound, #resources_generics_decl> {
                placement: Placement<'o>,

                state: parking_lot::Mutex<OperationState<(u64, #writes_name<'o, #(#write_types,)*>), #continuations_name<'o, #(#write_types,)*>, #downstreams_name<'o, #(#write_types,)*>>>,

                body: B,
                reads: UnsafeSyncCell<#reads_name<'o, #(#read_types,)*>>,
                grounding_result: UnsafeSyncCell<Option<InternalResult<Duration>>>
            }

            #[allow(clippy::unused_unit)]
            impl<'s, 'o: 's, B: #body_function_bound, #resources_generics_decl> #name<'o, B, #resources_generics_usage> {
                pub fn new(placement: Placement<'o>, body: B) -> Self {
                    #name {
                        state: Default::default(),
                        body,
                        reads: Default::default(),
                        grounding_result: UnsafeSyncCell::new(placement.get_static().map(Ok)),
                        placement,
                    }
                }
                fn run_continuations(&self, mut state: parking_lot::MutexGuard<OperationState<(u64, #writes_name<'o, #(#write_types,)*>), #continuations_name<'o, #(#write_types,)*>, #downstreams_name<'o, #(#write_types,)*>>>, scope: &rayon::Scope<'s>, timelines: &'s Timelines<'o>, env: ExecEnvironment<'s, 'o>) {
                    let mut swapped_continuations = smallvec::SmallVec::new();
                    std::mem::swap(&mut state.continuations, &mut swapped_continuations);
                    let output = state.status.unwrap_done();
                    drop(state);

                    let start_index = if env.stack_counter < STACK_LIMIT { 1 } else { 0 };

                    let time = unsafe {
                        (*self.grounding_result.get()).expect("expected grounding result to be present")
                    };

                    for c in swapped_continuations.drain(start_index..) {
                        match c {
                            #(#continuations_name::#writes(c) => {
                                scope.spawn(move |s| c.run(output.map(|r| (r.0, r.1.#writes)), s, timelines, env.reset()));
                            })*
                        }
                    }

                    if env.stack_counter < STACK_LIMIT {
                        match swapped_continuations.remove(0) {
                            #(#continuations_name::#writes(c) => {
                                c.run(output.map(|r| (r.0, r.1.#writes)), scope, timelines, env.increment());
                            })*
                        }
                    }
                }

                fn send_requests(&'o self, mut state: parking_lot::MutexGuard<OperationState<(u64, #writes_name<'o, #(#write_types,)*>), #continuations_name<'o, #(#write_types,)*>, #downstreams_name<'o, #(#write_types,)*>>>, time: Duration, scope: &rayon::Scope<'s>, timelines: &'s Timelines<'o>, env: ExecEnvironment<'s, 'o>) {
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
                            let continuation = Continuation::Node(self);
                            if num_requests == 0 && env.stack_counter < STACK_LIMIT {
                                #read_upstreams.expect("expected upstream to be present").request(continuation, already_registered, scope, timelines, env.increment());
                            } else {
                                scope.spawn(move |s| #read_upstreams.expect("expected upstream to be present").request(continuation, already_registered, s, timelines, env.reset()));
                            }
                        }
                    )*
                }

                fn run(&'o self, env: ExecEnvironment<'s, 'o>) -> InternalResult<(u64, #writes_name<'o, #(#write_types,)*>)> {
                    let reads = self.reads.get();

                    let (#((#read_response_hashes, #read_responses),)*) = unsafe {
                        (#((*reads).#read_responses.unwrap_or_else(|| panic!("expected response to be present: resource {}, node {:p}", #read_types::LABEL, self))?,)*)
                    };

                    let time_as_epoch = duration_to_epoch(
                        unsafe {
                            (*self.grounding_result.get()).expect("expected grounding result to be present").expect("expected grounding result to be ok")
                        }
                    );

                    let (#(#read_write_responses,)*) = (#(<#read_write_types as Resource>::Data::from_read(#read_write_responses, time_as_epoch),)*);
                    let (#(#read_only_responses,)*) = (#(<#read_only_types as Resource>::Data::sample(&#read_only_responses, time_as_epoch),)*);

                    let hash = {
                        use std::hash::{Hasher, BuildHasher, Hash};

                        let mut state = PeregrineDefaultHashBuilder::default();

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
                        ObservedErrorOutput
                    })
                }

                fn clear_cached_downstreams(&self) {
                    let mut state = self.state.lock();
                    match state.status {
                        OperationStatus::Dormant => {},
                        OperationStatus::Done(_) => {
                            state.status = OperationStatus::Dormant;
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

            impl<'o, B: #body_function_bound, #resources_generics_decl> NodeId for #name<'o, B, #resources_generics_usage> {
                const ID: u64 = peregrine_macros::random_u64!();
            }

            impl<'o, B: #body_function_bound, #resources_generics_decl> Node<'o> for #name<'o, B, #resources_generics_usage> {
                fn insert_self(&'o self, timelines: &Timelines<'o>, is_daemon: bool) -> Result<()> {
                    let notify_time = self.placement.min();
                    #(
                        let previous = timelines.insert::<#write_types>(self.placement, self, is_daemon);
                        assert!(!previous.is_empty());
                        for p in previous {
                            p.notify_downstreams(notify_time);
                        }
                    )*
                    Ok(())
                }
                fn remove_self(&self, timelines: &Timelines<'o>, is_daemon: bool) -> Result<()> {
                    #(
                        let removed = timelines.remove::<#write_types>(self.placement, is_daemon);
                        if !removed && !is_daemon {
                            bail!("Removal failed; could not find self at the expected time.")
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
            impl<'o, B: #body_function_bound, #resources_generics_decl R: Resource> Downstream<'o, R> for #name<'o, B, #resources_generics_usage> {
                fn respond<'s>(
                    &'o self,
                    value: InternalResult<(u64, <R::Data as Data<'o>>::Read)>,
                    scope: &rayon::Scope<'s>,
                    timelines: &'s Timelines<'o>,
                    env: ExecEnvironment<'s, 'o>
                ) where 'o: 's {
                    castaway::match_type!(R::INSTANCE, {
                        #(
                            #read_types as _ => {
                                assert!(
                                    std::mem::size_of::<<#read_types::Data as Data<'o>>::Read>()
                                        == std::mem::size_of::<<R::Data as Data<'o>>::Read>()
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
                        state.status = OperationStatus::Done(result);

                        self.run_continuations(state, scope, timelines, env);
                    }
                }

                fn clear_cache(&self) {
                    castaway::match_type!(R::INSTANCE, {
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

                fn clear_upstream(&self, time_of_change: Option<Duration>) -> bool {
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
                        castaway::match_type!(R::INSTANCE, {
                            #(
                                #read_types as _ => {
                                    unsafe {
                                        (*reads).#read_upstreams = None;
                                        (*reads).#read_responses = None;
                                    }
                                    <Self as Downstream::<'o, #read_types>>::clear_cache(self);
                                },
                            )*
                            _ => unreachable!()
                        });
                    }

                    retain
                }
            }

            impl<'o, B: #body_function_bound, #resources_generics_decl> GroundingDownstream<'o> for #name<'o, B, #resources_generics_usage> {
                fn respond_grounding<'s>(
                    &'o self,
                    value: InternalResult<(usize, Duration)>,
                    scope: &rayon::Scope<'s>,
                    timelines: &'s Timelines<'o>,
                    env: ExecEnvironment<'s, 'o>
                ) where 'o: 's {
                    unsafe {
                        (*self.grounding_result.get()) = Some(value.map(|r| r.1));
                    }

                    let mut state = self.state.lock();

                    match state.status {
                        OperationStatus::Dormant => {},
                        OperationStatus::Working => {
                            if let Ok((_, t)) = value {
                                if #num_reads == 0 {
                                    drop(state);
                                    let result = self.run(env);

                                    let mut state = self.state.lock();
                                    state.status = OperationStatus::Done(result);

                                    self.run_continuations(state, scope, timelines, env);
                                } else {
                                    self.send_requests(state, t, scope, timelines, env);
                                }
                            } else {
                                state.status = OperationStatus::Done(Err(ObservedErrorOutput));
                                self.run_continuations(state, scope, timelines, env);
                            }
                        }
                        OperationStatus::Done(_) => unreachable!()
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

            impl<'o, B: #body_function_bound, #resources_generics_decl R: Resource> Upstream<'o, R> for #name<'o, B, #resources_generics_usage> {
                fn request<'s>(
                    &'o self,
                    continuation: Continuation<'o, R>,
                    already_registered: bool,
                    scope: &rayon::Scope<'s>,
                    timelines: &'s Timelines<'o>,
                    env: ExecEnvironment<'s, 'o>
                ) where 'o: 's {
                    let mut state = self.state.lock();
                    if !already_registered {
                        if let Some(d) = continuation.to_downstream() {
                            castaway::match_type!(R::INSTANCE, {
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
                        OperationStatus::Dormant => {
                            castaway::match_type!(R::INSTANCE, {
                                #(
                                    #write_types as _ => state.continuations.push(#continuations_name::#writes(
                                        unsafe { std::mem::transmute(continuation) }
                                    )),
                                )*
                                _ => unreachable!()
                            });
                            state.status = OperationStatus::Working;
                            match self.placement.get_static() {
                                Some(t) => {
                                    if #num_reads == 0 {
                                        drop(state);
                                        let result = self.run(env);

                                        let mut state = self.state.lock();
                                        state.status = OperationStatus::Done(result);

                                        self.run_continuations(state, scope, timelines, env);
                                    } else {
                                        self.send_requests(state, t, scope, timelines, env);
                                    }
                                }
                                None => unsafe {
                                    match *self.grounding_result.get() {
                                        Some(Ok(t)) => self.send_requests(state, t, scope, timelines, env),
                                        Some(Err(_)) => {
                                            state.status = OperationStatus::Done(Err(ObservedErrorOutput));
                                            self.run_continuations(state, scope, timelines, env);
                                        }
                                        None => {
                                            drop(state);
                                            self.placement.request_grounding(GroundingContinuation::Node(0, self), false, scope, timelines, env.increment())
                                        }
                                    }
                                }
                            }
                        }
                        OperationStatus::Done(r) => {
                            drop(state);
                            let send = r.map(|o| {
                                let time = unsafe {
                                    (*self.grounding_result.get()).expect("expected grounding result to be present").expect("expected grounding result to be ok")
                                };
                                let value = castaway::match_type!(R::INSTANCE, {
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
                        OperationStatus::Working => {
                            castaway::match_type!(R::INSTANCE, {
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

                fn notify_downstreams(&self, time_of_change: Duration) {
                    let mut state = self.state.lock();

                    state.downstreams.retain(|downstream| {
                        match downstream {
                            #(
                                #downstreams_name::#writes(d) if castaway::cast!(R::INSTANCE, #write_types).is_ok() => d.clear_upstream(Some(time_of_change)),
                            )*
                            _ => true
                        }
                    });
                }

                fn register_downstream_early(&self, downstream: &'o dyn Downstream<'o, R>) {
                    let wrapped = castaway::match_type!(R::INSTANCE, {
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
                    continuation: GroundingContinuation<'o>,
                    already_registered: bool,
                    scope: &rayon::Scope<'s>,
                    timelines: &'s Timelines<'o>,
                    env: ExecEnvironment<'s, 'o>
                ) where 'o: 's {
                    self.placement.request_grounding(continuation, already_registered, scope, timelines, env);
                }
            }
        }
    }
}
