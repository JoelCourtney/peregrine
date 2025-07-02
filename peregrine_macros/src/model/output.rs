use crate::resource::Resource::Group;
use crate::resource::output::{
    generate_enum_name, generate_group_name, generate_member_resource_ident, generate_variant_name,
};
use crate::{
    model::{Daemon, Model},
    resource::GroupResource,
};
use proc_macro2::TokenStream;
use quote::{ToTokens, TokenStreamExt, format_ident, quote};

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
                .chain(Some(format_ident!(
                    "{}",
                    generate_group_name(&group.name_pattern)
                )))
                .collect(),
        });

        let resources = imported_resources
            .clone()
            .into_iter()
            .chain(new_resource_names.clone().map(|id| id.into()))
            .collect::<Vec<_>>();

        let mut daemons = daemons.clone();
        daemons.extend(new_resources.iter().flat_map(|r| match r {
            Group(GroupResource { name_pattern, members, ..}) => {
                let member_resources = members.iter().map(|m| generate_member_resource_ident(name_pattern, &m.to_string())).collect::<Vec<_>>();
                let group_ident = format_ident!("{}", crate::resource::output::generate_group_name(name_pattern));
                let enum_ident = format_ident!("{}", generate_enum_name(name_pattern));
                let mut result = member_resources.iter().zip(members).map(|(member_resource,member_variant) | {
                    let enum_variant = format_ident!("{}", generate_variant_name(&member_variant.to_string()));
                    Daemon {
                        resources: vec![syn::parse(member_resource.into_token_stream().into()).unwrap()],
                        function_call: syn::parse(quote! {peregrine::internal::resource::group::sync_single_to_group::<#group_ident,#member_resource,#enum_ident>(#enum_ident::#enum_variant)}.into()).expect("Could not generate single-to-group sync call"),
                        react_to_all: false,
                    }
                }).collect::<Vec<_>>();
                result.push(Daemon {
                    resources:  vec![syn::parse(group_ident.to_token_stream().into()).unwrap()],
                    function_call: syn::parse(quote! {
                        (|mut ops| {
                            ops += peregrine::op! {
                                #(m:#member_resources = m:#group_ident.#members;)*
                            }
                        })()
                    }.into()).unwrap(),
                    react_to_all: false
                });
                result
            }
            _ => vec![]
        }));

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
                    Box::new(move |placement, member| {
                        let result = std::cell::RefCell::new(vec![]);
                        let ops = peregrine::Ops::new(placement, &member, &result, new_order.clone());
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
                    timelines: &mut peregrine::internal::macro_prelude::Timelines<'o>,
                    order: std::sync::Arc<std::sync::atomic::AtomicU64>
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
                        let new_order = order.clone();
                        timelines.add_reactive_daemon(
                            peregrine::internal::macro_prelude::peregrine_macros::random_u64!(),
                            #daemons
                        );
                    )*

                    #(#sub_models::init_timelines(time, initial_conditions, timelines, order.clone())?;)*

                    Ok(())
                }
            }

            #(#new_resources)*
        };

        tokens.append_all(result);
    }
}
