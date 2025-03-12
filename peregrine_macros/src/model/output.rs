use crate::model::Model;
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
        } = self;

        let new_resource_visibilities = new_resources.iter().map(|r| r.0.clone());
        let new_resource_names = new_resources.iter().map(|r| r.1.clone());
        let new_resource_types = new_resources.iter().map(|r| r.2.clone());

        let resources = imported_resources
            .clone()
            .into_iter()
            .chain(new_resource_names.clone().map(|id| id.into()))
            .collect::<Vec<_>>();

        let result = quote! {
            #visibility enum #name {}

            impl<'o> peregrine::Model<'o> for #name {
                fn init_history(history: &mut peregrine::macro_prelude::History) {
                    #(history.init::<#resources>();)*
                }
                fn init_timelines<M: peregrine::macro_prelude::Model<'o>>(
                    time: peregrine::macro_prelude::Duration,
                    initial_conditions: &mut peregrine::macro_prelude::InitialConditions,
                    timelines: &mut peregrine::macro_prelude::Timelines<'o, M>
                ) {
                    #(
                        if !timelines.contains_resource::<#resources>() {
                            timelines.init_for_resource::<#resources>(
                                time,
                                peregrine::macro_prelude::InitialConditionOp::new(
                                    time,
                                    initial_conditions.take::<#resources>()
                                        .unwrap_or_else(|| panic!("expected to find initial condition for resource {}, but found none", <#resources as peregrine::macro_prelude::Resource>::LABEL))
                                )
                            );
                        }
                    )*

                    #(#sub_models::init_timelines::<M>(time, initial_conditions, timelines);)*
                }
            }

            peregrine::resource! {
                #(#new_resource_visibilities #new_resource_names: #new_resource_types,)*
            }
        };

        tokens.append_all(result);
    }
}
