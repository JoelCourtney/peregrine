use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Ident, Token, Type,
    parse::{Parse, ParseStream},
};

/// Parses the input for the sync! macro: `$model:ident : $t:ty => $plan:ident`
struct SyncInput {
    model: Ident,
    _colon: Token![:],
    ty: Type,
    _arrow: Token![=>],
    plan: Ident,
}

impl Parse for SyncInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(SyncInput {
            model: input.parse()?,
            _colon: input.parse()?,
            ty: input.parse()?,
            _arrow: input.parse()?,
            plan: input.parse()?,
        })
    }
}

pub fn sync_macro(input: TokenStream) -> TokenStream {
    let SyncInput {
        model, ty, plan, ..
    } = syn::parse_macro_input!(input as SyncInput);

    let expanded = quote! {
        let mut __peregrine_internal_sync_recorder = {
            use peregrine::undo::Undo;
            (#model).recorder()
        };
        let mut #model = <#ty>::chrono_recorder(&#plan.time_tracker, &mut __peregrine_internal_sync_recorder);
    };

    TokenStream::from(expanded)
}
