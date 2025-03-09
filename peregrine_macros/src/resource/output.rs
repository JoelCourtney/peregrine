use crate::resource::{HistoryType, Resource};
use proc_macro2::TokenStream;
use quote::{ToTokens, quote};

impl ToTokens for Resource {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Resource {
            visibility,
            name,
            write_type,
            history,
            dynamic,
        } = self;

        let label = name.to_string();

        let read_type = match history {
            HistoryType::Copy => quote! { #write_type },
            HistoryType::Deref => quote! { &'h <#write_type as std::ops::Deref>::Target },
        };

        let history_type = match history {
            HistoryType::Copy => quote! { peregrine::history::CopyHistory<#write_type> },
            HistoryType::Deref => quote! { peregrine::history::DerefHistory<#write_type> },
        };

        let sample_type = if !*dynamic {
            quote! { #read_type }
        } else {
            quote! { <#read_type as peregrine::resource::Dynamic>::Sample }
        };

        let sample_fn = if !*dynamic {
            quote! { *value }
        } else {
            quote! {
                **value
            }
        };

        let wrappers = if !*dynamic {
            quote! {
                type SendWrapper = Self::Read;
                type ReadWrapper = Self::Read;
                type WriteWrapper = Self::Write;
                fn wrap(value: Self::Read, _at: peregrine::Duration) -> Self::SendWrapper {
                    value
                }
                fn convert_for_reading(wrapped: Self::SendWrapper, _at: peregrine::Duration) -> Self::ReadWrapper {
                    wrapped
                }
                fn convert_for_writing(wrapped: Self::SendWrapper) -> Self::WriteWrapper {
                    wrapped.into()
                }
                fn unwrap_write(wrapped: Self::WriteWrapper) -> Self::Write {
                    wrapped
                }
                fn unwrap_read(wrapped: Self::SendWrapper) -> Self::Read {
                    wrapped
                }
            }
        } else {
            quote! {
                type SendWrapper = peregrine::resource::Timestamped<Self::Read>;
                type ReadWrapper = peregrine::resource::Sampled<Self::Read>;
                type WriteWrapper = peregrine::resource::Timestamped<Self::Write>;
                fn wrap(value: Self::Read, at: peregrine::Duration) -> Self::SendWrapper {
                    Self::SendWrapper::new(peregrine::timeline::duration_to_epoch(at), value)
                }
                fn convert_for_reading(wrapped: Self::SendWrapper, at: peregrine::Duration) -> Self::ReadWrapper {
                    Self::ReadWrapper::new_from(wrapped, peregrine::timeline::duration_to_epoch(at))
                }
                fn convert_for_writing(wrapped: Self::ReadWrapper) -> Self::WriteWrapper {
                    Self::WriteWrapper::new_from(wrapped)
                }
                fn unwrap_write(wrapped: Self::WriteWrapper) -> Self::Write {
                    wrapped.into_inner()
                }
                fn unwrap_read(wrapped: Self::SendWrapper) -> Self::Read {
                    wrapped.into_inner()
                }
            }
        };

        let result = quote! {
            #[derive(Debug, peregrine::reexports::serde::Serialize, peregrine::reexports::serde::Deserialize)]
            #[serde(crate = "peregrine::reexports::serde")]
            #[allow(non_camel_case_types)]
            #visibility enum #name {
                Unit
            }

            impl<'h> peregrine::resource::Resource<'h> for #name {
                const LABEL: &'static str = #label;
                const DYNAMIC: bool = #dynamic;
                const ID: u64 = peregrine::reexports::peregrine_macros::random_u64!();
                type Read = #read_type;
                type Write = #write_type;
                type History = #history_type;
                #wrappers
                type Sample = #sample_type;
                fn sample(value: &Self::ReadWrapper) -> #sample_type {
                    #sample_fn
                }
            }

            impl peregrine::resource::ResourceHistoryPlugin for #name {
                fn write_type_string(&self) -> String {
                    peregrine::reexports::peregrine_macros::code_to_str!(#write_type).to_string()
                }

                fn ser<'h>(&self, input: &'h peregrine::reexports::type_map::concurrent::TypeMap, type_map: &'h mut peregrine::reexports::type_reg::untagged::TypeMap<String>) {
                    if let Some(h) = input.get::<#history_type>() {
                        type_map.insert(self.write_type_string(), h.clone());
                    }
                }

                fn register(&self, type_reg: &mut peregrine::reexports::type_reg::untagged::TypeReg<String>) {
                    type_reg.register::<#history_type>(self.write_type_string());
                }
                fn de<'h>(&self, output: &'h mut peregrine::reexports::type_map::concurrent::TypeMap, type_map: &'h mut peregrine::reexports::type_reg::untagged::TypeMap<String>) {
                    match type_map.remove(&self.write_type_string()) {
                        Some(sub) => {
                            let sub_history = sub.into_inner().downcast::<#history_type>();
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
