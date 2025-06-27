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

        let new_resource_visibilities = new_resources.iter().map(|r| r.0.clone());
        let new_resource_names = new_resources.iter().map(|r| r.1.clone());
        let new_resource_types = new_resources.iter().map(|r| r.2.clone());

        let resources = imported_resources
            .clone()
            .into_iter()
            .chain(new_resource_names.clone().map(|id| id.into()))
            .collect::<Vec<_>>();

        let daemons = daemons.into_iter().map(|d| {
            let Daemon {
                resources,
                mut function_call,
            } = d.clone();

            function_call
                .args
                .insert(0, syn::Expr::Verbatim(quote!(ops)));

            quote! {
                peregrine::internal::macro_prelude::ReactiveDaemon::new(
                    vec![#(#resources::ID),*],
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
                ) {
                    use peregrine::Resource;
                    #(
                        if !timelines.contains_resource::<#resources>() {
                            timelines.init_for_resource::<#resources>(
                                time,
                                peregrine::internal::macro_prelude::InitialConditionOp::new(
                                    time,
                                    initial_conditions.take::<#resources>()
                                        .unwrap_or_else(|| panic!("expected to find initial condition for resource {}, but found none", <#resources as peregrine::Resource>::LABEL))
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

                    #(#sub_models::init_timelines(time, initial_conditions, timelines);)*
                }
            }

            peregrine::resource! {
                #(#new_resource_visibilities #new_resource_names: #new_resource_types,)*
            }
        };

        tokens.append_all(result);
    }
}
