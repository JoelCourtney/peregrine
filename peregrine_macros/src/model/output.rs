use crate::model::Model;
use proc_macro2::TokenStream;
use quote::{format_ident, quote, ToTokens, TokenStreamExt};

impl ToTokens for Model {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Model {
            visibility,
            name,
            resources,
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
                type Timelines = #timelines_struct_name<'o>;
                type InitialConditions = #initial_conditions_struct_name<'o>;

                fn init_history(history: &mut peregrine::history::History) {
                    #(history.init::<#resources>();)*
                }
            }

            #visibility struct #initial_conditions_struct_name<'h> {
                #(#resource_idents: <#resources as peregrine::resource::Resource<'h>>::Write,)*
            }

            #visibility struct #timelines_struct_name<'o> {
                #(#timeline_names: peregrine::timeline::Timeline<'o, #resources, #name>,)*
            }

            impl<'o> From<(peregrine::Duration, &'o peregrine::exec::SyncBump, #initial_conditions_struct_name<'o>)> for #timelines_struct_name<'o> {
                fn from((time, bump, inish_condish): (peregrine::Duration, &'o peregrine::exec::SyncBump, #initial_conditions_struct_name)) -> Self {
                    Self {
                        #(#timeline_names: peregrine::timeline::Timeline::<#resources, #name>::init(
                            time,
                            bump.alloc(peregrine::operation::InitialConditionOp::new(time, inish_condish.#resource_idents))
                        ),)*
                    }
                }
            }

            #(
                impl<'o> peregrine::timeline::HasTimeline<'o, #resources, #name> for #timelines_struct_name<'o> {
                    fn find_child(&self, time: peregrine::Duration) -> &'o (dyn peregrine::operation::Writer<'o, #resources, #name>) {
                        let (last_time, last_op) = self.#timeline_names.last();
                        if last_time < time {
                            last_op
                        } else {
                            self.#timeline_names.last_before(time).1
                        }
                    }
                    fn insert_operation(&mut self, time: peregrine::Duration, op: &'o dyn peregrine::operation::Writer<'o, #resources, #name>) -> &'o dyn peregrine::operation::Writer<'o, #resources, #name> {
                        self.#timeline_names.insert(time, op)
                    }
                    fn remove_operation(&mut self, time: peregrine::Duration) {
                        self.#timeline_names.remove(time);
                    }

                    fn get_operations(&self, bounds: impl std::ops::RangeBounds<peregrine::Duration>) -> Vec<(peregrine::Duration, &'o dyn peregrine::operation::Writer<'o, #resources, #name>)> {
                        self.#timeline_names.range(bounds).map(|(t,n)| (t, n)).collect()
                    }
                }
            )*
        };

        tokens.append_all(result);
    }
}
