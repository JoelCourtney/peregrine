use crate::resource::{GroupResource, MultiResource, Resource, SingleResource};
use quote::{ToTokens, format_ident, quote};
use syn::Ident;

/// Generate a single resource definition with the given name, data type, attributes, visibility, and default
fn generate_single_resource_definition(
    resource_name: &Ident,
    data_type: &syn::Type,
    attrs: &[syn::Attribute],
    visibility: &syn::Visibility,
    default_expr: Option<&syn::Expr>,
) -> proc_macro2::TokenStream {
    let default_impl = if let Some(default) = default_expr {
        quote! { Some(#default) }
    } else {
        quote! { None }
    };

    quote! {
        #(#attrs)*
        #[derive(Copy, Clone)]
        #[allow(non_camel_case_types)]
        #visibility enum #resource_name {
            Unit
        }

        impl peregrine::public::resource::Resource for #resource_name {
            const LABEL: &'static str = peregrine::internal::macro_prelude::peregrine_macros::code_to_str!(#resource_name);
            const ID: u64 = peregrine::internal::macro_prelude::peregrine_macros::random_u64!();
            type Data = #data_type;
            const INSTANCE: Self = Self::Unit;

            fn initial_condition() -> Option<Self::Data> {
                #default_impl
            }
        }

        impl peregrine::internal::resource::ResourceHistoryPlugin for #resource_name {
            fn write_type_string(&self) -> String {
                peregrine::internal::macro_prelude::peregrine_macros::code_to_str!(#resource_name).to_string()
            }

            fn ser<'h>(&self, input: &'h peregrine::internal::macro_prelude::type_map::concurrent::TypeMap, type_map: &'h mut peregrine::internal::macro_prelude::type_reg::untagged::TypeMap<String>) {
                if let Some(h) = input.get::<peregrine::internal::history::InnerHistory<#resource_name>>() {
                    type_map.insert(self.write_type_string(), h.clone());
                }
            }

            fn register(&self, type_reg: &mut peregrine::internal::macro_prelude::type_reg::untagged::TypeReg<String>) {
                type_reg.register::<peregrine::internal::history::InnerHistory<#resource_name>>(self.write_type_string());
            }
            fn de<'h>(&self, output: &'h mut peregrine::internal::macro_prelude::type_map::concurrent::TypeMap, type_map: &'h mut peregrine::internal::macro_prelude::type_reg::untagged::TypeMap<String>) {
                match type_map.remove(&self.write_type_string()) {
                    Some(sub) => {
                        let sub_history = sub.into_inner().downcast::<peregrine::internal::history::InnerHistory<#resource_name>>();
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

        peregrine::internal::macro_prelude::inventory::submit!(&(#resource_name::Unit) as &dyn peregrine::internal::resource::ResourceHistoryPlugin);
    }
}

impl ToTokens for SingleResource {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let resource_def = generate_single_resource_definition(
            &self.name,
            &self.data_type,
            &self.attrs,
            &self.visibility,
            self.default_expr.as_ref(),
        );
        tokens.extend(resource_def);
    }
}

impl ToTokens for GroupResource {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        // Expand resource group into individual resources
        for member in &self.members {
            let member_name_string = self.name_pattern.replace('*', &member.to_string());
            let member_name = format_ident!("{}", member_name_string);

            // Determine the default expression for this member
            let member_default = if let Some(individual_default) =
                self.individual_defaults.get(&member.to_string())
            {
                Some(individual_default)
            } else {
                self.default_expr.as_ref()
            };

            let resource_def = generate_single_resource_definition(
                &member_name,
                &self.data_type,
                &self.attrs,
                &self.visibility,
                member_default,
            );
            tokens.extend(resource_def);
        }
    }
}

impl ToTokens for Resource {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        match self {
            Resource::Single(single) => single.to_tokens(tokens),
            Resource::Group(group) => group.to_tokens(tokens),
        }
    }
}

impl ToTokens for MultiResource {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        for resource in &self.resources {
            resource.to_tokens(tokens);
        }
    }
}
