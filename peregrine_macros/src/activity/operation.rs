use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use std::collections::HashMap;

pub(crate) fn process_operation(input: String) -> TokenStream {
    let mut writes = HashMap::new();
    let mut read_writes = HashMap::new();

    let activity_start = input
        .find("activity")
        .expect("could not find activity label")
        + 8;
    let activity_end = input[activity_start..]
        .find(';')
        .expect("could not find activity end")
        + activity_start;

    let activity = format_ident!("{}", input[activity_start..activity_end].trim());

    let reads_start = input.find("reads").expect("could not find reads start") + 5;
    let reads_end = input[reads_start..]
        .find(';')
        .expect("could not find reads end")
        + reads_start;

    let temp_reads = input[reads_start..reads_end]
        .split(',')
        .map(|s| {
            let colon = s.find(':');
            match colon {
                None => panic!("no colon in read"),
                Some(c) => {
                    let name = format_ident!("{}", s[..c].trim());
                    let path: TokenStream = s[c + 1..]
                        .parse()
                        .expect("could not parse read resource type path");
                    (name, path)
                }
            }
        })
        .collect::<HashMap<_, _>>();

    let writes_start = input.find("writes").expect("could not find writes start") + 6;
    let writes_end = input[writes_start..]
        .find(";")
        .expect("could not find writes end")
        + writes_start;

    input[writes_start..writes_end].split(',').for_each(|s| {
        let colon = s.find(':');
        match colon {
            None => {
                let name = format_ident!("{}", s.trim());
                match temp_reads.get(&name) {
                    None => panic!("write variable doesn't have a resource type: {name}"),
                    Some(path) => read_writes.insert(name, path.clone()),
                };
            }
            Some(c) => {
                let name = format_ident!("{}", s[..c].trim());
                let path: TokenStream = s[c + 1..]
                    .parse()
                    .expect("could not parse write resource type path");
                if temp_reads.contains_key(&name) {
                    read_writes.insert(name, path);
                } else {
                    writes.insert(name, path);
                }
            }
        }
    });

    let reads = temp_reads
        .into_iter()
        .filter(|(n, _)| !read_writes.contains_key(n))
        .collect();

    let uuid = uuid::Uuid::new_v4().to_string().replace("-", "_");
    let output_ident = format_ident!("{activity}OpOutput_{uuid}");
    let op = format_ident!("{activity}Op_{uuid}");
    let op_relationships = format_ident!("{activity}OpRelationships_{uuid}");

    let idents = Idents {
        op_relationships,
        op,
        output: output_ident,
        activity,
        reads,
        writes,
        read_writes,
    };

    let when_start = input.find("when").expect("could not find when start") + 4;
    let when_end = input[when_start..]
        .find(';')
        .expect("could not find when end")
        + when_start;
    let when: TokenStream = input[when_start..when_end]
        .parse()
        .expect("could not parse when clause");

    let op_start = input.find("op").expect("could not find op start") + 2;
    let operation_body: TokenStream = input[op_start..]
        .to_string()
        .parse()
        .expect("could not parse op body");
    let op = generate_operation(&idents, operation_body);

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

struct Idents {
    op_relationships: Ident,
    op: Ident,
    output: Ident,
    activity: Ident,
    reads: HashMap<Ident, TokenStream>,
    writes: HashMap<Ident, TokenStream>,
    read_writes: HashMap<Ident, TokenStream>,
}

fn generate_operation(idents: &Idents, body: TokenStream) -> TokenStream {
    let (read_only_variables, read_only_paths) = idents.reads.iter().collect::<(Vec<_>, Vec<_>)>();
    let (write_only_variables, write_only_paths) =
        idents.writes.iter().collect::<(Vec<_>, Vec<_>)>();
    let (read_write_variables, read_write_paths) =
        idents.read_writes.iter().collect::<(Vec<_>, Vec<_>)>();

    let all_paths = read_only_paths
        .iter()
        .chain(write_only_paths.iter())
        .chain(read_write_paths.iter())
        .collect::<Vec<_>>();

    let all_read_variables = read_only_variables
        .iter()
        .chain(read_write_variables.iter())
        .collect::<Vec<_>>();
    let all_write_variables = write_only_variables
        .iter()
        .chain(read_write_variables.iter())
        .collect::<Vec<_>>();

    let all_read_paths = read_only_paths
        .iter()
        .chain(read_write_paths.iter())
        .collect::<Vec<_>>();
    let all_write_paths = write_only_paths
        .iter()
        .chain(read_write_paths.iter())
        .collect::<Vec<_>>();

    let first_write_variable = all_write_variables[0];
    let all_but_one_write_variables = &all_write_variables[1..];

    let first_write_path = all_write_paths[0];
    let all_but_one_write_paths = &all_write_paths[1..];

    let read_only_resource_hashes = read_only_variables
        .iter()
        .map(|i| format_ident!("_peregrine_engine_resource_hash_{i}"))
        .collect::<Vec<_>>();
    let read_write_resource_hashes = read_write_variables
        .iter()
        .map(|i| format_ident!("_peregrine_engine_resource_hash_{i}"))
        .collect::<Vec<_>>();
    let all_read_resource_hashes = all_read_variables
        .iter()
        .map(|i| format_ident!("_peregrine_engine_resource_hash_{i}"))
        .collect::<Vec<_>>();

    let Idents {
        op_relationships,
        op,
        output,
        activity,
        ..
    } = idents;

    let run_internal = quote! {
        let new_env = env.increment();

        let relationships = self.relationships.blocking_lock();

        #(let (#read_only_resource_hashes, #read_only_variables) = relationships.#read_only_variables
                .read(history, new_env)
                .await;
        )*

        #(
            let (#read_write_resource_hashes, mut #read_write_variables): (u64, <#read_write_paths as peregrine::Resource<'o>>::Write) = {
                let (hash, #read_write_variables) = relationships.#read_write_variables
                    .read(history, new_env)
                    .await;
                (hash, (*#read_write_variables).into())
            };
        )*

        std::mem::drop(relationships);

        #(let mut #write_only_variables = <#write_only_paths as peregrine::Resource<'o>>::Write::default();)*

        let hash = {
            use std::hash::{Hasher, BuildHasher, Hash};

            let mut state = peregrine::history::PeregrineDefaultHashBuilder::default().build_hasher();
            std::any::TypeId::of::<#output>().hash(&mut state);

            #(#all_read_resource_hashes.hash(&mut state);)*

            state.finish()
        };

        let (#(#all_write_variables),*) = if let Some(#first_write_variable) = history.get::<#first_write_path>(hash) {
            #(let #all_but_one_write_variables = history.get::<#all_but_one_write_paths>(hash).unwrap();)*
            (#(#all_write_variables),*)
        } else {
            let args = self.activity;
            { #body }
            #(let #all_write_variables = history.insert::<#all_write_paths>(hash, #all_write_variables);)*
            (#(#all_write_variables),*)
        };

        #(drop(#read_only_variables);)*

        Some(#output {
            hash,
            #(#all_write_variables,)*
        })
    };

    let timelines_bound = quote! {
        M::Timelines: #(peregrine::timeline::HasTimeline<'o, #all_paths, M>)+*
    };

    quote! {
        struct #op_relationships<'o, M: peregrine::Model<'o>> {
            parents: Vec<&'o dyn peregrine::operation::Operation<'o, M>>,
            #(#all_read_variables: &'o dyn peregrine::operation::Writer<'o, #all_read_paths, M>,)*

        }

        struct #op<'o, M: peregrine::Model<'o>> {
            result: peregrine::reexports::tokio::sync::RwLock<Option<#output<'o>>>,
            relationships: peregrine::reexports::tokio::sync::Mutex<#op_relationships<'o, M>>,
            activity: &'o #activity,
            time: peregrine::Duration,
        }

        impl<'o, M: peregrine::Model<'o>> peregrine::operation::Operation<'o, M> for #op<'o, M>
        where #timelines_bound {
            fn find_children(&'o self, time_of_change: peregrine::Duration, timelines: &M::Timelines) {
                if time_of_change >= self.time { return; }

                let mut changed = false;

                let mut lock = self.relationships.blocking_lock();
                #(
                    let new_child = <M::Timelines as peregrine::timeline::HasTimeline<'o, #all_read_paths, M>>::find_child(timelines, self.time);
                    if !std::ptr::eq(new_child, lock.#all_read_variables) {
                        lock.#all_read_variables.remove_parent(self);
                        new_child.add_parent(self);
                        lock.#all_read_variables = new_child;
                        changed = true;
                    }
                )*

                drop(lock);

                if changed {
                    let mut queue = std::collections::VecDeque::<&'o dyn peregrine::operation::Operation<'o, M>>::new();
                    queue.push_back(self);
                    while let Some(op) = queue.pop_front() {
                        if op.clear_cache() {
                            queue.extend(op.parents());
                        }
                    }
                }
            }
            fn add_parent(&self, parent: &'o dyn peregrine::operation::Operation<'o, M>) {
                self.relationships.blocking_lock().parents.push(parent);
            }
            fn remove_parent(&self, parent: &dyn peregrine::operation::Operation<'o, M>) {
                self.relationships.blocking_lock().parents.retain(|p| !std::ptr::eq(*p, parent));
            }

            fn insert_self(&'o self, timelines: &mut M::Timelines) {
                #(
                    let previous = <M::Timelines as peregrine::timeline::HasTimeline<#all_write_paths, M>>::insert_operation(timelines, self.time, self);
                    previous.notify_parents(self.time, timelines);
                )*
                let lock = self.relationships.blocking_lock();
                #(lock.#all_read_variables.add_parent(self);)*
            }
            fn remove_self(&self, timelines: &mut M::Timelines) {
                #(
                    <M::Timelines as peregrine::timeline::HasTimeline<#all_write_paths, M>>::remove_operation(timelines, self.time);
                )*
                self.notify_parents(self.time, timelines);
                let lock = self.relationships.blocking_lock();
                #(lock.#all_read_variables.remove_parent(self);)*
            }

            fn parents(&self) -> Vec<&'o dyn peregrine::operation::Operation<'o, M>> {
                self.relationships.blocking_lock().parents.clone()
            }
            fn notify_parents(&self, time_of_change: peregrine::Duration, timelines: &M::Timelines) {
                let lock = self.relationships.blocking_lock();
                let parents = lock.parents.clone();
                drop(lock);
                for parent in parents {
                    parent.find_children(time_of_change, timelines);
                }
            }
            fn clear_cache(&self) -> bool {
                self.result.blocking_write().take().is_some()
            }
        }

        #(
            impl<'o, M: peregrine::Model<'o>> peregrine::operation::Writer<'o, #all_write_paths, M> for #op<'o, M>
            where #timelines_bound {
                fn read<'b>(&'o self, history: &'o peregrine::History, env: peregrine::exec::ExecEnvironment<'b>) -> peregrine::exec::BumpedFuture<'b, (u64, peregrine::reexports::tokio::sync::RwLockReadGuard<'o, <#all_write_paths as peregrine::Resource<'o>>::Read>)> where 'o: 'b {
                    unsafe { std::pin::Pin::new_unchecked(env.bump.alloc(async move {
                        // If you (the thread) can get the write lock on the node, then you are responsible
                        // for calculating the hash and value if they aren't present.
                        // Otherwise, wait for a read lock and return the cached results.
                        let read: peregrine::reexports::tokio::sync::RwLockReadGuard<_> = if let Ok(mut write) = self.result.try_write() {
                            if write.is_none() {
                                let result = if env.should_spawn == peregrine::exec::ShouldSpawn::Yes {
                                    peregrine::exec::EXECUTOR.spawn_scoped(async move {
                                        let new_bump = peregrine::exec::SyncBump::new();
                                        let env = peregrine::exec::ExecEnvironment::new(&new_bump);
                                        #run_internal
                                    }).await
                                } else {
                                    #run_internal
                                };
                                *write = result;
                                write.downgrade()
                            } else {
                                write.downgrade()
                            }
                        } else {
                            self.result.read().await
                        };

                        (
                            read.as_ref().unwrap().hash,
                            peregrine::reexports::tokio::sync::RwLockReadGuard::map(read, |o| &o.as_ref().unwrap().#all_write_variables)
                        )
                    }))}
                }
            }
        )*
    }
}

fn generate_output(idents: &Idents) -> TokenStream {
    let (write_only_variables, write_only_paths) =
        idents.writes.iter().collect::<(Vec<_>, Vec<_>)>();
    let (read_write_variables, read_write_paths) =
        idents.read_writes.iter().collect::<(Vec<_>, Vec<_>)>();

    let all_write_variables = write_only_variables
        .iter()
        .chain(read_write_variables.iter())
        .collect::<Vec<_>>();

    let all_write_paths = write_only_paths
        .iter()
        .chain(read_write_paths.iter())
        .collect::<Vec<_>>();

    let Idents { output, .. } = idents;
    quote! {
        #[derive(Clone, Default)]
        struct #output<'h> {
            hash: u64,
            #(#all_write_variables: <#all_write_paths as peregrine::Resource<'h>>::Read,)*
        }
    }
}

fn result(idents: &Idents, when: TokenStream) -> TokenStream {
    let Idents {
        op,
        op_relationships,
        ..
    } = idents;

    let (read_only_variables, read_only_paths) = idents.reads.iter().collect::<(Vec<_>, Vec<_>)>();
    let (read_write_variables, read_write_paths) =
        idents.read_writes.iter().collect::<(Vec<_>, Vec<_>)>();

    let all_read_variables = read_only_variables
        .iter()
        .chain(read_write_variables.iter())
        .collect::<Vec<_>>();

    let all_read_paths = read_only_paths
        .iter()
        .chain(read_write_paths.iter())
        .collect::<Vec<_>>();

    quote! {
        {
            let when = peregrine::timeline::epoch_to_duration(#when);

            let op = bump.alloc(#op {
                result: peregrine::reexports::tokio::sync::RwLock::new(None),
                activity: &self,
                relationships: peregrine::reexports::tokio::sync::Mutex::new(#op_relationships {
                    parents: Vec::with_capacity(2),
                    #(#all_read_variables: <M::Timelines as peregrine::timeline::HasTimeline<#all_read_paths, M>>::find_child(timelines, when),)*
                }),
                time: when
            });

            operations.push(op);
        }
    }
}
