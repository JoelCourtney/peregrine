use crate::model::{Daemon, Model};
use proc_macro2::TokenStream;
use quote::{ToTokens, TokenStreamExt, quote};

impl ToTokens for Model {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Model {
            visibility,
            name,
            imported_resources,
            new_resources,
            sub_models,
            daemons,
        } = self;

        let new_resource_entries = new_resources.iter().flat_map(|r| {
            match r {
                crate::resource::Resource::Single(single) => {
                    let vis = &single.visibility;
                    let name = &single.name;
                    let ty = &single.data_type;
                    let default = &single.default_expr;

                    vec![match default {
                        Some(expr) => quote! { #vis #name: #ty = #expr; },
                        None => quote! { #vis #name: #ty; },
                    }]
                }
                crate::resource::Resource::Group(group) => {
                    group
                        .members
                        .iter()
                        .map(|member| {
                            let vis = &group.visibility;
                            let member_name_string =
                                group.name_pattern.replace('*', &member.to_string());
                            let member_name = quote::format_ident!("{}", member_name_string);
                            let ty = &group.data_type;

                            // Determine the default expression for this member
                            let member_default = if let Some(individual_default) =
                                group.individual_defaults.get(&member.to_string())
                            {
                                Some(individual_default)
                            } else {
                                group.default_expr.as_ref()
                            };

                            match member_default {
                                Some(expr) => quote! { #vis #member_name: #ty = #expr; },
                                None => quote! { #vis #member_name: #ty; },
                            }
                        })
                        .collect()
                }
            }
        });

        let new_resource_names = new_resources.iter().flat_map(|r| match r {
            crate::resource::Resource::Single(single) => {
                vec![single.name.clone()]
            }
            crate::resource::Resource::Group(group) => group
                .members
                .iter()
                .map(|member| {
                    let member_name_string = group.name_pattern.replace('*', &member.to_string());
                    quote::format_ident!("{}", member_name_string)
                })
                .collect(),
        });

        let resources = imported_resources
            .clone()
            .into_iter()
            .chain(new_resource_names.clone().map(|id| id.into()))
            .collect::<Vec<_>>();

        let daemons = daemons.iter().map(|d| {
            let Daemon {
                resources: daemon_resources,
                mut function_call,
                react_to_all,
            } = d.clone();

            function_call
                .args
                .insert(0, syn::Expr::Verbatim(quote!(ops)));

            // Use all resources if react_to_all is true, otherwise use the specified resources
            let resource_ids = if react_to_all {
                quote! { vec![#(#resources::ID),*] }
            } else {
                quote! { vec![#(#daemon_resources::ID),*] }
            };

            quote! {
                peregrine::internal::macro_prelude::ReactiveDaemon::new(
                    #resource_ids,
                    Box::new(|placement, member| {
                        let result = std::cell::RefCell::new(vec![]);
                        let ops = peregrine::Ops::new(placement, &member, &result);
                        #function_call;
                        result.into_inner()
                    })
                )
            }
        });

        let result = quote! {
            #visibility enum #name {}

            impl<'o> peregrine::Model<'o> for #name {
                fn init_history(history: &mut peregrine::internal::macro_prelude::History) {
                    #(history.init::<#resources>();)*
                    #(#sub_models::init_history(history);)*
                }
                fn init_timelines(
                    time: peregrine::Duration,
                    initial_conditions: &mut peregrine::internal::macro_prelude::InitialConditions,
                    timelines: &mut peregrine::internal::macro_prelude::Timelines<'o>
                ) -> peregrine::anyhow::Result<()> {
                    use peregrine::Resource;
                    #(
                        if !timelines.contains_resource::<#resources>() {
                            let initial_value = match initial_conditions.take::<#resources>() {
                                Some(value) => value,
                                None => if let Some(def) = <#resources as peregrine::Resource>::initial_condition() {
                                    def
                                } else {
                                    let type_default = peregrine::internal::macro_prelude::spez::spez! {
                                        for #resources::Unit;
                                        match<T: peregrine::Resource> T where T::Data: Default -> Option<T::Data> {
                                            Some(T::Data::default())
                                        }
                                        match<T> T -> Option<<#resources as peregrine::Resource>::Data> {
                                            None
                                        }
                                    };
                                    if let Some(td) = type_default {
                                        td
                                    } else {
                                        peregrine::anyhow::bail!("No initial condition provided for resource {}.\nEither implement Default or provide a value to initial_conditions! or resource!/model!.", #resources::LABEL)
                                    }
                                }
                            };
                            timelines.init_for_resource::<#resources>(
                                time,
                                peregrine::internal::macro_prelude::InitialConditionOp::new(
                                    time,
                                    initial_value
                                )
                            );
                        }
                    )*

                    #(
                        timelines.add_reactive_daemon(
                            peregrine::internal::macro_prelude::peregrine_macros::random_u64!(),
                            #daemons
                        );
                    )*

                    #(#sub_models::init_timelines(time, initial_conditions, timelines)?;)*

                    Ok(())
                }
            }

            peregrine::resource! {
                #(#new_resource_entries)*
            }
        };

        tokens.append_all(result);
    }
}
