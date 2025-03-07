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

        let resource_idents = resources
            .iter()
            .map(|r| {
                format_ident!(
                    "{}",
                    r.into_token_stream()
                        .to_string()
                        .chars()
                        .filter(|c| c.is_alphanumeric() || *c == '_')
                        .collect::<String>()
                )
            })
            .collect::<Vec<_>>();

        let timeline_names = resource_idents
            .iter()
            .map(|i| format_ident!("{}_operation_timeline", i))
            .collect::<Vec<_>>();

        let timelines_struct_name = format_ident!("{name}Timelines");
        let initial_conditions_struct_name = format_ident!("{name}InitialConditions");

        let result = quote! {
            #visibility enum #name {}

            impl<'o> peregrine::Model<'o> for #name {
                fn init_history(history: &mut peregrine::history::History) {
                    #(history.init::<#resources>();)*
                }
                fn init_timelines(time: peregrine::Duration, mut initial_conditions: peregrine::operation::initial_conditions::InitialConditions, herd: &'o peregrine::reexports::bumpalo_herd::Herd) -> peregrine::timeline::Timelines<'o, Self> {
                    let mut timelines = peregrine::timeline::Timelines::new(herd);
                    #(timelines.init_for_resource::<#resources>(time, peregrine::operation::initial_conditions::InitialConditionOp::new(time, initial_conditions.take::<#resources>().expect(&format!("expected to find initial condition for resource {}, but found none", <#resources as peregrine::resource::Resource<'o>>::LABEL))));)*
                    timelines
                }
            }

            #visibility struct #initial_conditions_struct_name<'h> {
                #(#resource_idents: <#resources as peregrine::resource::Resource<'h>>::Write,)*
            }

            #visibility struct #timelines_struct_name<'o> {
                #(#timeline_names: peregrine::timeline::Timeline<'o, #resources, #name>,)*
            }

            impl<'o> From<(peregrine::Duration, &peregrine::reexports::bumpalo_herd::Member<'o>, #initial_conditions_struct_name<'o>)> for #timelines_struct_name<'o> {
                fn from((time, bump, inish_condish): (peregrine::Duration, &peregrine::reexports::bumpalo_herd::Member<'o>, #initial_conditions_struct_name)) -> Self {
                    Self {
                        #(#timeline_names: peregrine::timeline::Timeline::<#resources, #name>::init(
                            time,
                            bump.alloc(peregrine::operation::initial_conditions::InitialConditionOp::<'o, #resources, #name>::new(time, inish_condish.#resource_idents))
                        ),)*
                    }
                }
            }
        };

        tokens.append_all(result);
    }
}
