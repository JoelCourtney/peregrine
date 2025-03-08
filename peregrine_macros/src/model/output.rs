use crate::model::Model;
use proc_macro2::TokenStream;
use quote::{ToTokens, TokenStreamExt, format_ident, quote};

impl ToTokens for Model {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Model {
            visibility,
            name,
            resources,
            ..
        } = self;

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
                    fn init_timelines(
                        time: Duration,
                        mut initial_conditions: InitialConditions,
                        herd: &'o bumpalo_herd::Herd
                    ) -> Timelines<'o, Self> {
                        let mut timelines = Timelines::new(herd);
                        #(
                            timelines.init_for_resource::<#resources>(
                                time,
                                InitialConditionOp::new(
                                    time,
                                    initial_conditions.take::<#resources>()
                                        .unwrap_or_else(|| panic!("expected to find initial condition for resource {}, but found none", <#resources as Resource<'o>>::LABEL))
                                )
                            );
                        )*
                        timelines
                    }
                }
            }
        };

        tokens.append_all(result);
    }
}
