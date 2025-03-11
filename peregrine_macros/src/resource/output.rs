use crate::resource::Resource;
use proc_macro2::TokenStream;
use quote::{ToTokens, quote};

impl ToTokens for Resource {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Resource {
            visibility,
            name,
            write_type,
        } = self;

        let label = name.to_string();

        let result = quote! {
            #[derive(Debug, peregrine::reexports::serde::Serialize, peregrine::reexports::serde::Deserialize)]
            #[serde(crate = "peregrine::reexports::serde")]
            #[allow(non_camel_case_types)]
            #visibility enum #name {
                Unit
            }

            impl peregrine::resource::Resource for #name {
                const LABEL: &'static str = #label;
                const ID: u64 = peregrine::reexports::peregrine_macros::random_u64!();
                type Data = #write_type;
            }

            impl peregrine::resource::ResourceHistoryPlugin for #name {
                fn write_type_string(&self) -> String {
                    peregrine::reexports::peregrine_macros::code_to_str!(#write_type).to_string()
                }

                fn ser<'h>(&self, input: &'h peregrine::reexports::type_map::concurrent::TypeMap, type_map: &'h mut peregrine::reexports::type_reg::untagged::TypeMap<String>) {
                    if let Some(h) = input.get::<peregrine::history::InnerHistory<#write_type>>() {
                        type_map.insert(self.write_type_string(), h.clone());
                    }
                }

                fn register(&self, type_reg: &mut peregrine::reexports::type_reg::untagged::TypeReg<String>) {
                    type_reg.register::<peregrine::history::InnerHistory<#write_type>>(self.write_type_string());
                }
                fn de<'h>(&self, output: &'h mut peregrine::reexports::type_map::concurrent::TypeMap, type_map: &'h mut peregrine::reexports::type_reg::untagged::TypeMap<String>) {
                    match type_map.remove(&self.write_type_string()) {
                        Some(sub) => {
                            let sub_history = sub.into_inner().downcast::<peregrine::history::InnerHistory<#write_type>>();
                            match sub_history {
                                Ok(downcasted) => {
                                    output.insert(*downcasted);
                                }
                                Err(_) => unreachable!()
                            }
                        }
                        None => {}
                    }
                }
            }

            peregrine::reexports::inventory::submit!(&#name::Unit as &dyn peregrine::resource::ResourceHistoryPlugin);
        };

        tokens.extend(result);
    }
}
