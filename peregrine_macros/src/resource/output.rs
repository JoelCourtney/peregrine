use crate::resource::{MultiResource, Resource};
use quote::{ToTokens, quote};

impl ToTokens for Resource {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let Resource {
            visibility,
            name,
            data_type,
            default_expr,
            attrs,
        } = self;

        let default_impl = if let Some(default) = default_expr {
            quote! { Some(#default) }
        } else {
            quote! { None }
        };

        let expanded = quote! {
            #(#attrs)*
            #[derive(Copy, Clone)]
            #[allow(non_camel_case_types)]
            #visibility enum #name {
                Unit
            }

            impl peregrine::public::resource::Resource for #name {
                const LABEL: &'static str = peregrine::internal::macro_prelude::peregrine_macros::code_to_str!(#name);
                const ID: u64 = peregrine::internal::macro_prelude::peregrine_macros::random_u64!();
                type Data = #data_type;
                const INSTANCE: Self = Self::Unit;

                fn initial_condition() -> Option<Self::Data> {
                    #default_impl
                }
            }

            impl peregrine::internal::resource::ResourceHistoryPlugin for #name {
                fn write_type_string(&self) -> String {
                    peregrine::internal::macro_prelude::peregrine_macros::code_to_str!(#name).to_string()
                }

                fn ser<'h>(&self, input: &'h peregrine::internal::macro_prelude::type_map::concurrent::TypeMap, type_map: &'h mut peregrine::internal::macro_prelude::type_reg::untagged::TypeMap<String>) {
                    if let Some(h) = input.get::<peregrine::internal::history::InnerHistory<#name>>() {
                        type_map.insert(self.write_type_string(), h.clone());
                    }
                }

                fn register(&self, type_reg: &mut peregrine::internal::macro_prelude::type_reg::untagged::TypeReg<String>) {
                    type_reg.register::<peregrine::internal::history::InnerHistory<#name>>(self.write_type_string());
                }
                fn de<'h>(&self, output: &'h mut peregrine::internal::macro_prelude::type_map::concurrent::TypeMap, type_map: &'h mut peregrine::internal::macro_prelude::type_reg::untagged::TypeMap<String>) {
                    match type_map.remove(&self.write_type_string()) {
                        Some(sub) => {
                            let sub_history = sub.into_inner().downcast::<peregrine::internal::history::InnerHistory<#name>>();
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

            peregrine::internal::macro_prelude::inventory::submit!(&(#name::Unit) as &dyn peregrine::internal::resource::ResourceHistoryPlugin);
        };

        tokens.extend(expanded);
    }
}

impl ToTokens for MultiResource {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        for resource in &self.resources {
            resource.to_tokens(tokens);
        }
    }
}
