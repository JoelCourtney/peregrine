use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Data, DeriveInput, Error, Fields, FnArg, ImplItem, ImplItemFn, ItemImpl, parse_macro_input,
};

pub fn derive_chronological(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    // Validate that this is a struct
    let Data::Struct(data) = input.data else {
        return Error::new_spanned(input, "Chronological can only be derived for structs")
            .to_compile_error()
            .into();
    };

    let struct_name = &input.ident;
    let visibility = &input.vis;
    let generics = &input.generics;

    let chrono_recorder_name = format_ident!("{}ChronoRecorder", &struct_name);

    // Extract only the generic parameters (without bounds)
    let generic_params = &generics.params;

    // Process fields
    let (chrono_recorder_fields, chrono_recorder_inits) = match &data.fields {
        Fields::Named(fields) => {
            let mut chrono_recorder_fields = Vec::new();
            let mut chrono_recorder_inits = Vec::new();

            for (index, field) in fields.named.iter().enumerate() {
                let field_type = &field.ty;
                let field_vis = &field.vis;
                let field_name = if let Some(i) = &field.ident {
                    i.clone()
                } else {
                    format_ident!("{index}")
                };

                // Check if field has #[no_undo] or #[no_chrono] attribute
                let has_no_undo_attr = field
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident("no_undo"));
                let has_no_chrono_attr = field
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident("no_chrono"));

                if !has_no_undo_attr && !has_no_chrono_attr {
                    // Field is not excluded
                    chrono_recorder_fields.push(quote! {
                        #field_vis #field_name: <#field_type as peregrine::plan::Chronological>::ChronoRecorder<'r, 'i>
                    });

                    // Add chrono_recorder initialization
                    chrono_recorder_inits.push(quote! {
                        #field_name: <#field_type as peregrine::plan::Chronological>::chrono_recorder(
                            time,
                            &mut recorder.#field_name
                        )
                    });
                }
            }

            (chrono_recorder_fields, chrono_recorder_inits)
        }
        Fields::Unnamed(_) => {
            return Error::new_spanned(
                data.fields,
                "Chronological derive macro does not support tuple structs",
            )
            .to_compile_error()
            .into();
        }
        Fields::Unit => {
            return Error::new_spanned(
                data.fields,
                "Chronological derive macro does not support unit structs",
            )
            .to_compile_error()
            .into();
        }
    };

    // Split generics for impl block
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let expanded = quote! {
        #visibility struct #chrono_recorder_name<'r, 'i, #ty_generics> where 'i: 'r #where_clause {
            time_tracker: std::rc::Rc<std::cell::Cell<peregrine::plan::Time>>,
            #(#chrono_recorder_fields,)*
        }

        impl<'r, 'i, #generic_params> peregrine::plan::ErasedChronoRecorder for #chrono_recorder_name<'r, 'i, #generic_params> where 'i: 'r #where_clause {}

        impl #impl_generics peregrine::plan::Chronological for #struct_name #ty_generics #where_clause {
            type ChronoRecorder<'r, 'i> = #chrono_recorder_name<'r, 'i, #generic_params>
            where
                Self: 'r + 'i, 'i: 'r;

            fn chrono_recorder<'r, 'i>(
                time: &std::rc::Rc<std::cell::Cell<peregrine::plan::Time>>,
                recorder: &'r mut Self::Recorder<'i>,
            ) -> Self::ChronoRecorder<'r, 'i> where 'i: 'r {
                #chrono_recorder_name {
                    time_tracker: time.clone(),
                    #(#chrono_recorder_inits,)*
                }
            }
        }
    };

    TokenStream::from(expanded)
}

pub fn chronological_attribute(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut impl_block = parse_macro_input!(item as ItemImpl);

    // Extract the self type
    let self_ty = &impl_block.self_ty;
    let generics = &impl_block.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Check if this is a trait impl
    let is_trait_impl = impl_block.trait_.is_some();

    // Collect instance methods and check for associated functions
    let mut recorder_methods = Vec::new();
    let mut has_associated_functions = false;

    for item in &mut impl_block.items {
        if let ImplItem::Fn(method) = item {
            // Check if this is an instance method by looking for a self parameter
            let has_self = method
                .sig
                .inputs
                .iter()
                .any(|arg| matches!(arg, FnArg::Receiver(_)));

            if has_self {
                // Process this method for sync annotations BEFORE stripping attributes
                match process_method(method, is_trait_impl) {
                    Ok(methods) => recorder_methods.extend(methods),
                    Err(e) => return e.to_compile_error().into(),
                }

                // Strip #[sync] attributes from the original method
                for arg in &mut method.sig.inputs {
                    if let FnArg::Typed(pat_type) = arg {
                        pat_type.attrs.retain(|attr| !attr.path().is_ident("sync"));
                    }
                }
            } else {
                has_associated_functions = true;
            }
        }
    }

    // If this is a trait impl and has associated functions, error out
    if is_trait_impl && has_associated_functions {
        return Error::new_spanned(
            impl_block,
            "chronological attribute cannot be applied to trait impl blocks with associated functions",
        )
        .to_compile_error()
        .into();
    }

    // Generate the ChronoRecorder type name
    let recorder_type_name = if let syn::Type::Path(type_path) = &**self_ty {
        if let Some(last_segment) = type_path.path.segments.last() {
            format_ident!("{}ChronoRecorder", last_segment.ident)
        } else {
            return Error::new_spanned(self_ty, "Cannot determine type name")
                .to_compile_error()
                .into();
        }
    } else {
        return Error::new_spanned(self_ty, "Expected a path type")
            .to_compile_error()
            .into();
    };

    // Generate the ChronoRecorder impl block
    let trait_part = if let Some((bang, path, for_token)) = &impl_block.trait_ {
        // This is a trait impl
        quote! { #bang #path #for_token }
    } else {
        quote! {}
    };

    let recorder_impl = if is_trait_impl {
        quote! {
            impl #impl_generics #trait_part #recorder_type_name<'_, '_, #ty_generics> #where_clause {
                #(#recorder_methods)*
            }
        }
    } else {
        quote! {
            impl #impl_generics #recorder_type_name<'_, '_, #ty_generics> #where_clause {
                #(#recorder_methods)*
            }
        }
    };

    // Output both the original impl block and the new ChronoRecorder impl
    let expanded = quote! {
        #impl_block

        #recorder_impl
    };

    TokenStream::from(expanded)
}

fn process_method(method: &ImplItemFn, is_trait_impl: bool) -> Result<Vec<ImplItemFn>, Error> {
    // Check if any arguments have #[sync] attribute
    let sync_args: Vec<_> = method
        .sig
        .inputs
        .iter()
        .enumerate()
        .filter_map(|(idx, arg)| {
            if let FnArg::Typed(pat_type) = arg {
                let has_sync = pat_type
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident("sync"));
                if has_sync { Some(idx) } else { None }
            } else {
                None
            }
        })
        .collect();

    // If there are sync args in a trait impl, error out
    if is_trait_impl && !sync_args.is_empty() {
        return Err(Error::new_spanned(
            &method.sig,
            "#[sync] annotations cannot be used in trait impl blocks",
        ));
    }

    let mut methods = Vec::new();

    if sync_args.is_empty() {
        // No sync args, just clone the method as-is
        methods.push(method.clone());
    } else {
        // Create the synced version (without sync args)
        let mut synced_method = method.clone();
        let mut sync_arg_names = Vec::new();
        let mut sync_arg_types = Vec::new();

        // Remove sync args from the signature and collect their names
        let mut new_inputs = vec![];
        for (idx, arg) in method.sig.inputs.iter().enumerate() {
            if sync_args.contains(&idx) {
                if let FnArg::Typed(pat_type) = arg {
                    sync_arg_names.push(pat_type.pat.clone());
                    sync_arg_types.push(pat_type.ty.clone());
                }
            } else {
                // Keep non-sync args, but remove #[sync] attribute if present
                let mut arg = arg.clone();
                if let FnArg::Typed(ref mut pat_type) = arg {
                    pat_type.attrs.retain(|attr| !attr.path().is_ident("sync"));
                }
                new_inputs.push(arg);
            }
        }

        synced_method.sig.inputs = new_inputs.into_iter().collect();

        // Add local variable assignments at the start of the method body
        let original_block = &method.block;
        let sync_var_inits = sync_arg_names.iter().map(|name| {
            quote! {
                let #name = self.time_tracker.get();
            }
        });

        synced_method.block = syn::parse2(quote! {
            {
                #(#sync_var_inits)*
                #original_block
            }
        })
        .unwrap();

        methods.push(synced_method);

        // Create the unsync version with original signature
        let mut unsync_method = method.clone();
        let unsync_name = format_ident!("{}_at", method.sig.ident);
        unsync_method.sig.ident = unsync_name;

        // Remove #[sync] attributes from all arguments
        unsync_method.sig.inputs = method
            .sig
            .inputs
            .iter()
            .map(|arg| {
                let mut arg = arg.clone();
                if let FnArg::Typed(ref mut pat_type) = arg {
                    pat_type.attrs.retain(|attr| !attr.path().is_ident("sync"));
                }
                arg
            })
            .collect();

        methods.push(unsync_method);
    }

    Ok(methods)
}
