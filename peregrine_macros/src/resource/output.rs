use crate::resource::{GroupResource, MultiResource, Resource, SingleResource};
use heck::ToUpperCamelCase;
use quote::{ToTokens, format_ident, quote};
use syn::{Expr, Ident};

/// Generate an enum name from a resource group pattern
/// e.g., "heater_*_active" -> "HeaterActive"
/// e.g., "*_pump_enabled" -> "PumpEnabled"
/// e.g., "thruster_*" -> "Thruster"
pub fn generate_enum_name(pattern: &str) -> String {
    let cleaned = generate_group_name(pattern);

    // Convert to PascalCase
    cleaned
        .split('_')
        .filter(|s| !s.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
            }
        })
        .collect()
}

pub fn generate_group_name(pattern: &str) -> String {
    // Remove the asterisk
    let without_asterisk = pattern.replace('*', "");

    // Clean up multiple consecutive underscores
    without_asterisk
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

/// Convert a member name to UpperCamelCase for enum variants
/// e.g., "main" -> "Main", "a" -> "A"
pub fn generate_variant_name(member: &str) -> String {
    member.to_upper_camel_case()
}

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
        // Generate the group enum first
        let enum_name_string = generate_enum_name(&self.name_pattern);
        let enum_name = format_ident!("{}", enum_name_string);
        let visibility = &self.visibility;

        // Generate enum variants from member names
        let variants: Vec<_> = self
            .members
            .iter()
            .map(|member| {
                let variant_name = generate_variant_name(&member.to_string());
                format_ident!("{}", variant_name)
            })
            .collect();

        // Generate the enum definition
        let enum_def = quote! {
            #[derive(
                peregrine::internal::macro_prelude::variants_struct::VariantsStruct,
                Copy,
                Clone,
                Eq,
                PartialEq,
                Debug,
                peregrine::internal::macro_prelude::enum_iterator::Sequence,
                std::hash::Hash
            )]
            #[struct_derive(Clone, Debug, peregrine::Data, peregrine::MaybeHash, peregrine::internal::macro_prelude::serde::Serialize, peregrine::internal::macro_prelude::serde::Deserialize)]
            #[struct_bounds(for<'his> peregrine::Data<'his>)]
            #[struct_attr(serde(bound(serialize = "T: for<'his> peregrine::Data<'his>")))]
            #[struct_attr(serde(bound(deserialize = "T: for<'his> peregrine::Data<'his>")))]
            #visibility enum #enum_name {
                #(#variants,)*
            }
        };

        tokens.extend(enum_def);

        let group_name = format_ident!("{}", generate_group_name(&self.name_pattern));
        let struct_name = format_ident!("{}Struct", &enum_name);
        let member_data_type = &self.data_type;
        let group_type = syn::Type::Verbatim(quote! { #struct_name<#member_data_type> });

        let group_default: Option<Expr> = if let Some(d) = &self.default_expr {
            let members = &self.members;
            Some(syn::parse(quote! { #struct_name { #(#members: #d),*}}.into()).unwrap())
        } else if !self.individual_defaults.is_empty() {
            let mut members = vec![];
            let mut exprs = vec![];
            for (member, expr) in &self.individual_defaults {
                members.push(format_ident!("{}", member));
                exprs.push(expr);
            }
            Some(syn::parse(quote! { #struct_name { #(#members: #exprs),* }}.into()).unwrap())
        } else {
            None
        };

        tokens.extend(generate_single_resource_definition(
            &group_name,
            &group_type,
            &self.attrs,
            &self.visibility,
            group_default.as_ref(),
        ));

        // Expand resource group into individual resources
        for member in &self.members {
            let member_name =
                generate_member_resource_ident(&self.name_pattern, &member.to_string());

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

pub fn generate_member_resource_ident(group_pattern: &str, member: &str) -> Ident {
    format_ident!("{}", group_pattern.replace('*', member))
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
