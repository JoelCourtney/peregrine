use crate::model::Model;
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
        } = self;

        let new_resource_visibilities = new_resources.iter().map(|r| r.0.clone());
        let new_resource_names = new_resources.iter().map(|r| r.1.clone());
        let new_resource_types = new_resources.iter().map(|r| r.2.clone());

        let resources = imported_resources
            .clone()
            .into_iter()
            .chain(new_resource_names.clone().map(|id| id.into()))
            .collect::<Vec<_>>();

        let mod_name = format_ident!("peregrine_model_mod_{name}");

        let result = quote! {
            #visibility enum #name {}

            #visibility mod #mod_name {
                use super::*;
                use peregrine::macro_prelude::*;
                impl<'o> Model<'o> for #name {
                    fn init_history(history: &mut History) {
                        #(history.init::<#resources>();)*
                    }
                    fn init_timelines<M: Model<'o>>(
                        time: Duration,
                        initial_conditions: &mut InitialConditions,
                        timelines: &mut Timelines<'o, M>
                    ) {
                        #(
                            if !timelines.contains_resource::<#resources>() {
                                timelines.init_for_resource::<#resources>(
                                    time,
                                    InitialConditionOp::new(
                                        time,
                                        initial_conditions.take::<#resources>()
                                            .unwrap_or_else(|| panic!("expected to find initial condition for resource {}, but found none", <#resources as Resource>::LABEL))
                                    )
                                );
                            }
                        )*

                        #(#sub_models::init_timelines::<M>(time, initial_conditions, timelines);)*
                    }
                }
            }

            peregrine::resource! {
                #(#new_resource_visibilities #new_resource_names: #new_resource_types,)*
            }
        };

        tokens.append_all(result);
    }
}
