use crate::operation::Op;
use crate::{
    MAX_PREGENERATED_ORDER, impl_node, impl_read_structs_internal, impl_write_structs_internal,
};
use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, format_ident, quote};

impl Op {
    fn body_function(&self) -> TokenStream {
        let Idents {
            all_writes,
            write_onlys,
            read_onlys,
            read_writes,
            ..
        } = self.make_idents();

        let body = &self.body;

        let (crate_name, fn_name) = if self.internal {
            (quote! { crate }, quote! { FnInternal })
        } else {
            (quote! { peregrine }, quote! { Fn })
        };

        quote! {
            #crate_name::internal::macro_prelude::serde_closure::#fn_name!(move |#(#read_onlys: <<#read_onlys as #crate_name::Resource>::Data as #crate_name::Data>::Sample,)*
            #(mut #read_writes: <#read_writes as #crate_name::Resource>::Data,)*|
            -> #crate_name::anyhow::Result<(#(<#all_writes as #crate_name::Resource>::Data,)*)> {
                #(#[allow(unused_mut)] let mut #write_onlys: <#write_onlys as #crate_name::Resource>::Data;)*
                #body
                Ok((#(#all_writes,)*))
            })
        }
    }

    fn make_idents(&self) -> Idents {
        let Op {
            reads,
            writes,
            read_writes,
            ..
        } = self;

        Idents {
            write_onlys: writes.clone(),
            read_onlys: reads.clone(),
            read_writes: read_writes.clone(),
            all_writes: writes.iter().chain(read_writes.iter()).cloned().collect(),
        }
    }
}

impl ToTokens for Op {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let idents = self.make_idents();

        let num_read_onlys = idents.read_onlys.len();
        let num_read_writes = idents.read_writes.len();
        let num_write_onlys = idents.write_onlys.len();

        let mut declarations = quote! {};
        let mut empty_declaration = true;
        if (num_read_onlys + num_read_writes) as i32 > MAX_PREGENERATED_ORDER * 2 {
            let read_impls = impl_read_structs_internal(num_read_onlys + num_read_writes);
            declarations = quote! {
                #declarations
                #read_impls
            };
            empty_declaration = false;
        }
        if (num_write_onlys + num_read_writes) as i32 > MAX_PREGENERATED_ORDER * 2 {
            let write_impls = impl_write_structs_internal(num_write_onlys + num_read_writes);
            declarations = quote! {
                #declarations
                #write_impls
            };
            empty_declaration = false;
        }
        if num_read_onlys as i32 > MAX_PREGENERATED_ORDER
            || num_read_writes as i32 > MAX_PREGENERATED_ORDER
            || num_write_onlys as i32 > MAX_PREGENERATED_ORDER
        {
            let node_impl = impl_node(num_read_onlys, num_read_writes, num_write_onlys);
            declarations = quote! {
                #declarations
                #node_impl
            };
            empty_declaration = false;
        }

        let crate_name = if self.internal {
            quote! { crate }
        } else {
            quote! { peregrine }
        };

        let mod_name = if !empty_declaration {
            quote! { local_module:: }
        } else {
            quote! { #crate_name::internal::macro_prelude:: }
        };

        let instantiation = result(&idents, self.body_function(), mod_name);

        let result = quote! {
            {
                mod local_module {
                    use #crate_name as peregrine;
                    use peregrine::*;
                    use peregrine::internal::macro_prelude::*;
                    #declarations
                }
                #instantiation
            }
        };

        tokens.extend(result);
    }
}

struct Idents {
    read_onlys: Vec<Ident>,
    write_onlys: Vec<Ident>,
    read_writes: Vec<Ident>,
    all_writes: Vec<Ident>,
}

fn result(idents: &Idents, body_function: TokenStream, mod_name: TokenStream) -> TokenStream {
    let Idents {
        read_onlys,
        write_onlys,
        read_writes,
        ..
    } = idents;

    let num_read_onlys = read_onlys.len();
    let num_write_onlys = write_onlys.len();
    let num_read_writes = read_writes.len();

    let op_name = format_ident!("NodeR{num_read_onlys}Rw{num_read_writes}W{num_write_onlys}");

    let resources_generics = quote! {
        #(#read_onlys,)* #(#write_onlys,)* #(#read_writes,)*
    };

    quote! {
        move |placement| #mod_name #op_name::<'_,_, #resources_generics>::new(placement, #body_function)
    }
}
