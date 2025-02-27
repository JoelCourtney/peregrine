use crate::activity::Op;
use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, TokenStreamExt, format_ident, quote};
use syn::Expr;

impl Op {
    pub(crate) fn body_function(&self) -> TokenStream {
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
            activity,
            reads,
            writes,
            read_writes,
            uuid,
            ..
        } = self;

        let activity = activity.clone().expect("activity name was not set");

        let output = format_ident!("{activity}OpOutput_{uuid}");
        let op = format_ident!("{activity}Op_{uuid}");
        let op_relationships = format_ident!("{activity}OpRelationships_{uuid}");
        let op_body_function = format_ident!("{activity}_op_body_{uuid}");
        let continuations = format_ident!("{activity}Continuations_{uuid}");

        Idents {
            op_relationships,
            op,
            output,
            continuations,
            op_body_function,
            activity,
            write_onlys: writes.clone(),
            read_writes: read_writes.clone(),
            all_reads: reads.iter().chain(read_writes.iter()).cloned().collect(),
            all_writes: writes.iter().chain(read_writes.iter()).cloned().collect(),
            all_resources: reads
                .iter()
                .chain(writes.iter())
                .chain(read_writes.iter())
                .cloned()
                .collect(),
        }
    }
}

impl ToTokens for Op {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let idents = self.make_idents();

        let result = process_operation(idents, &self.when);
        tokens.append_all(result);
    }
}

struct Idents {
    op_relationships: Ident,
    op: Ident,
    output: Ident,
    op_body_function: Ident,
    continuations: Ident,
    activity: Ident,
    write_onlys: Vec<Ident>,
    read_writes: Vec<Ident>,
    all_reads: Vec<Ident>,
    all_writes: Vec<Ident>,
    all_resources: Vec<Ident>,
}

fn process_operation(idents: Idents, when: &Expr) -> TokenStream {
    let op = generate_operation(&idents);

    let output_struct = generate_output(&idents);

    let result = result(&idents, when);

    quote! {
        {
            #op
            #output_struct
            #result
        }
    }
}

fn generate_operation(idents: &Idents) -> TokenStream {
    let Idents {
        op_relationships,
        op,
        output,
        op_body_function,
        continuations,
        activity,
        all_reads,
        all_writes,
        all_resources,
        ..
    } = idents;

    let first_write = &all_writes[0];
    let all_but_one_write = &all_writes[1..];
    let first_read = &all_reads[0];
    let all_but_one_read = &all_reads[1..];

    let all_read_response_hashes = all_reads
        .iter()
        .map(|i| format_ident!("_peregrine_engine_resource_hash_{i}"))
        .collect::<Vec<_>>();

    let all_read_responses = all_reads
        .iter()
        .map(|i| format_ident!("{i}_response"))
        .collect::<Vec<_>>();

    let num_reads = all_reads.len() as u8;

    let timelines_bound = quote! {
        M::Timelines: #(peregrine::timeline::HasTimeline<'o, #all_resources, M>)+*
    };

    quote! {
        struct #op_relationships<'o, M: peregrine::Model<'o>> {
            downstreams: peregrine::operation::NodeVec<'o, M>,
            #(#all_reads: &'o dyn peregrine::operation::Upstream<'o, #all_reads, M>,)*
            #(#all_read_responses: peregrine::operation::InternalResult<(u64, <#all_reads as peregrine::resource::Resource<'o>>::Read)>,)*
        }

        struct #op<'o, M: peregrine::Model<'o>> {
            state: peregrine::reexports::crossbeam::atomic::AtomicCell<peregrine::operation::OperationState>,
            result: peregrine::operation::UnsyncUnsafeCell<peregrine::operation::InternalResult<#output<'o>>>,
            relationships: peregrine::reexports::parking_lot::Mutex<#op_relationships<'o, M>>,
            activity: &'o #activity,
            time: peregrine::Duration,
            continuations: peregrine::reexports::parking_lot::Mutex<peregrine::reexports::smallvec::SmallVec<#continuations<'o, M>, 1>>,
            response_counter: peregrine::reexports::crossbeam::atomic::AtomicCell<u8>
        }

        #[allow(non_camel_case_types)]
        enum #continuations<'o, M: peregrine::Model<'o>> {
            #(#all_writes(peregrine::operation::Continuation<'o, #all_writes, M>),)*
        }

        impl<'s, 'o: 's, M: peregrine::Model<'o>> #op<'o, M> where #timelines_bound {
            fn run_continuations(&self, scope: &peregrine::reexports::rayon::Scope<'s>, env: peregrine::exec::ExecEnvironment<'s, 'o>) {
                let result = unsafe {
                    *self.result.get()
                };
                let mut continuations = {
                    let mut result = peregrine::reexports::smallvec::SmallVec::new();
                    let mut lock = self.continuations.lock();
                    std::mem::swap(&mut result, &mut lock);
                    result
                };

                if !continuations.is_empty() {
                    let start_index = if env.stack_counter < peregrine::exec::STACK_LIMIT { 1 } else { 0 };

                    for c in continuations.drain(start_index..) {
                        match c {
                            #(#continuations::#all_writes(c) => scope.spawn(move |s| c.run(result.map(|r| (r.hash, r.#all_writes)), s, env.reset()) ),)*
                        }
                    }

                    if env.stack_counter < peregrine::exec::STACK_LIMIT {
                        match continuations.remove(0) {
                            #(#continuations::#all_writes(c) => c.run(result.map(|r| (r.hash, r.#all_writes)), scope, env.increment()),)*
                        }
                    }
                }
            }

            fn send_requests(&'o self, mut relationships: peregrine::reexports::parking_lot::MutexGuard<'o, #op_relationships<'o, M>>, scope: &peregrine::reexports::rayon::Scope<'s>, env: peregrine::exec::ExecEnvironment<'s, 'o>) {
                let (#(#all_reads,)*) = (#(relationships.#all_reads,)*);
                drop(relationships);

                self.response_counter.store(#num_reads);
                #(scope.spawn(move |s| #all_but_one_read.request(peregrine::operation::Continuation::Node(self), s, env.reset()));)*

                if env.stack_counter < peregrine::exec::STACK_LIMIT {
                    #first_read.request(peregrine::operation::Continuation::Node(self), scope, env.increment());
                } else {
                    scope.spawn(move |s| #first_read.request(peregrine::operation::Continuation::Node(self), s, env.increment()));
                }
            }

            fn run(&'o self, relationships_lock: peregrine::reexports::parking_lot::MutexGuard<'o, #op_relationships<'o, M>>, env: peregrine::exec::ExecEnvironment<'s, 'o>) -> peregrine::operation::InternalResult<#output<'o>> {
                use peregrine::{ActivityLabel, Context};

                let (#((#all_read_response_hashes, #all_reads),)*) = (#(relationships_lock.#all_read_responses?,)*);
                drop(relationships_lock);

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
                    self.activity.#op_body_function(#(#all_reads,)*)
                        .with_context(|| format!("occurred in activity {} at {}", #activity::LABEL, self.time))
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
        }

        impl<'o, M: peregrine::Model<'o>> peregrine::operation::Node<'o, M> for #op<'o, M>
        where #timelines_bound {
            fn find_upstreams(&'o self, time_of_change: peregrine::Duration, timelines: &M::Timelines) {
                if time_of_change >= self.time { return; }

                let mut changed = false;

                let mut lock = self.relationships.lock();
                #(
                    let new_child = <M::Timelines as peregrine::timeline::HasTimeline<'o, #all_reads, M>>::find_child(timelines, self.time).expect("unreachable; could not find a child that was previously there.");
                    if !std::ptr::eq(new_child, lock.#all_reads) {
                        lock.#all_reads.remove_downstream(self);
                        new_child.add_downstream(self);
                        lock.#all_reads = new_child;
                        changed = true;
                    }
                )*

                drop(lock);

                if changed {
                    let mut queue = std::collections::VecDeque::<&'o dyn peregrine::operation::Node<'o, M>>::new();
                    queue.push_back(self);
                    while let Some(op) = queue.pop_front() {
                        if op.clear_cache() {
                            queue.extend(op.downstreams());
                        }
                    }
                }
            }
            fn add_downstream(&self, downstream: &'o dyn peregrine::operation::Node<'o, M>) {
                self.relationships.lock().downstreams.push(downstream);
            }
            fn remove_downstream(&self, downstream: &dyn peregrine::operation::Node<'o, M>) {
                self.relationships.lock().downstreams.retain(|p| !std::ptr::eq(*p, downstream));
            }

            fn insert_self(&'o self, timelines: &mut M::Timelines) -> peregrine::Result<()> {
                #(
                    let previous = <M::Timelines as peregrine::timeline::HasTimeline<#all_writes, M>>::insert_operation(timelines, self.time, self)
                        .ok_or_else(|| peregrine::anyhow!("Could not find an upstream node. Did you insert before the initial conditions?"))?;
                    previous.notify_downstreams(self.time, timelines);
                )*
                let lock = self.relationships.lock();
                #(lock.#all_reads.add_downstream(self);)*
                Ok(())
            }
            fn remove_self(&self, timelines: &mut M::Timelines) -> peregrine::Result<()> {
                #(
                    let this = <M::Timelines as peregrine::timeline::HasTimeline<#all_writes, M>>::remove_operation(timelines, self.time);
                    if this.is_none() {
                        peregrine::bail!("Removal failed; could not find self at the expected time.")
                    }
                )*
                self.notify_downstreams(self.time, timelines);
                let lock = self.relationships.lock();
                #(lock.#all_reads.remove_downstream(self);)*
                Ok(())
            }

            fn downstreams(&self) -> peregrine::operation::NodeVec<'o, M> {
                self.relationships.lock().downstreams.clone()
            }
            fn clear_cache(&self) -> bool {
                use peregrine::operation::OperationState;

                match self.state.swap(OperationState::Dormant) {
                    OperationState::Dormant => false,
                    OperationState::Done => true,
                    OperationState::Waiting => unreachable!()
                }
            }
        }

        #(
            impl<'o, M: peregrine::Model<'o>> peregrine::operation::Downstream<'o, #all_reads, M> for #op<'o, M>
            where #timelines_bound {
                fn respond<'s>(&'o self, value: peregrine::operation::InternalResult<(u64, <#all_reads as peregrine::resource::Resource<'o>>::Read)>, scope: &peregrine::reexports::rayon::Scope<'s>, env: peregrine::exec::ExecEnvironment<'s, 'o>) where 'o: 's {
                    debug_assert_eq!(OperationState::Waiting, self.state.load());

                    use peregrine::operation::OperationState;
                    use peregrine::ActivityLabel;

                    let mut relationships_lock = self.relationships.lock();
                    relationships_lock.#all_read_responses = value;

                    if self.response_counter.fetch_sub(1) == 1 {
                        unsafe {
                            *self.result.get() = self.run(relationships_lock, env);
                        }

                        // Its important that we set state to Done after the value is computed
                        // but BEFORE continuations are run, to prevent race condition.
                        // Better to have two tasks cooperatively working through the continuation queue
                        // with some contention than to accidentally leave a continuation due to race conditions.
                        self.state.store(OperationState::Done);

                        self.run_continuations(scope, env);
                    }
                }
            }
        )*

        #(
            impl<'o, M: peregrine::Model<'o>> peregrine::operation::Upstream<'o, #all_writes, M> for #op<'o, M>
            where #timelines_bound {
                fn request<'s>(&'o self, continuation: peregrine::operation::Continuation<'o, #all_writes, M>, scope: &peregrine::reexports::rayon::Scope<'s>, env: peregrine::exec::ExecEnvironment<'s, 'o>) where 'o: 's {
                    use peregrine::operation::OperationState;

                    self.continuations.lock().push(#continuations::#all_writes(continuation));

                    match self.state.load() {
                        OperationState::Done => {
                            self.run_continuations(scope, env);
                        }
                        OperationState::Dormant => {
                            if let Some(relationships) = self.relationships.try_lock() {
                                self.state.store(OperationState::Waiting);
                                self.send_requests(relationships, scope, env);
                            }
                        }
                        OperationState::Waiting => {}
                    }
                }
            }
        )*
    }
}

fn generate_output(idents: &Idents) -> TokenStream {
    let Idents {
        all_writes, output, ..
    } = idents;

    quote! {
        #[derive(Copy, Clone, Default)]
        struct #output<'h> {
            hash: u64,
            #(#all_writes: <#all_writes as peregrine::resource::Resource<'h>>::Read,)*
        }
    }
}

fn result(idents: &Idents, when: &Expr) -> TokenStream {
    let Idents {
        op,
        op_relationships,
        all_reads,
        ..
    } = idents;

    let all_read_responses = all_reads
        .iter()
        .map(|i| format_ident!("{i}_response"))
        .collect::<Vec<_>>();

    quote! {
        {
            let when = peregrine::timeline::epoch_to_duration(#when);

            let op = bump.alloc(#op {
                state: peregrine::reexports::crossbeam::atomic::AtomicCell::new(peregrine::operation::OperationState::Dormant),
                result: peregrine::operation::UnsyncUnsafeCell::new(Err(peregrine::operation::ObservedErrorOutput)),
                continuations: Default::default(),
                response_counter: peregrine::reexports::crossbeam::atomic::AtomicCell::new(0),
                activity: &self,
                relationships: peregrine::reexports::parking_lot::Mutex::new(#op_relationships {
                    downstreams: peregrine::operation::NodeVec::new(),
                    #(
                        #all_reads: <M::Timelines as peregrine::timeline::HasTimeline<#all_reads, M>>::find_child(
                            timelines,
                            when
                        ).ok_or_else(|| peregrine::anyhow!("Could not find upstream node. Did you insert before the initial conditions?"))?,
                    )*
                    #(#all_read_responses: Err(peregrine::operation::ObservedErrorOutput),)*
                }),
                time: when,
            });

            operations.push(op);
        }
    }
}
