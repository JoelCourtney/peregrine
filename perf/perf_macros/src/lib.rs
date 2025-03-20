use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::{format_ident, quote};
use std::fmt::Display;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{LitInt, Token, parse_macro_input};

fn resource(i: impl Display) -> Ident {
    format_ident!("res_{i:0>3}")
}

fn activity(i: impl Display) -> Ident {
    format_ident!("Act{i:0>3}")
}

#[proc_macro]
pub fn declare_model(input: TokenStream) -> TokenStream {
    let num_resources = parse_macro_input!(input as LitInt)
        .base10_parse::<usize>()
        .unwrap();
    let mut resources = quote! {};
    for i in 0..num_resources {
        let ident = resource(i);
        resources = quote! {
            #resources
            #ident: i64,
        };
    }
    quote! {
        peregrine::model! {
            Perf { #resources }
        }
    }
    .into()
}

#[proc_macro]
pub fn declare_activities(input: TokenStream) -> TokenStream {
    let parser = Punctuated::<LitInt, Token![,]>::parse_terminated;
    let args = parser.parse(input).unwrap();
    let num_resources = args.get(0).unwrap().base10_parse::<usize>().unwrap();
    let num_activities = args.get(1).unwrap().base10_parse::<usize>().unwrap();
    let num_operations = args.get(2).unwrap().base10_parse::<usize>().unwrap();
    let spread = args.get(3).unwrap().base10_parse::<usize>().unwrap();

    let mut result = quote! {};
    for a in 0..num_activities {
        let mut activity_result = quote! {};
        for _ in 0..num_operations {
            let mut reads = vec![0u32; spread];
            rand::fill(&mut reads[..]);
            let mut writes = vec![0u32; spread];
            rand::fill(&mut writes[..]);

            let reads = reads
                .into_iter()
                .map(|i| resource(i % num_resources as u32))
                .collect::<Vec<_>>();
            let writes = writes
                .into_iter()
                .map(|i| resource(i % num_resources as u32))
                .collect::<Vec<_>>();

            activity_result = quote! {
                #activity_result
                ops += peregrine::op! {
                    let sum = #(ref: #reads)+*;
                    let mut counter = 0;
                    #(
                        mut: #writes = sum - counter;
                        counter += 1;
                    )*
                };
                ops.wait(1.seconds());
            };
        }
        let ident = activity(a);
        result = quote! {
            #result
            #[derive(Serialize, Deserialize)]
            struct #ident;

            #[typetag::serde]
            impl peregrine::Activity for #ident {
                fn run(&self, mut ops: peregrine::Ops) -> peregrine::Result<peregrine::Duration> {
                    use peregrine::reexports::hifitime::TimeUnits;
                    use peregrine::activity::OpsReceiver;
                    #activity_result
                    Ok((#num_operations as i64).seconds())
                }
            }
        }
    }

    result.into()
}

#[proc_macro]
pub fn make_initial_conditions(input: TokenStream) -> TokenStream {
    let num_resources = parse_macro_input!(input as LitInt)
        .base10_parse::<usize>()
        .unwrap();

    let mut result = quote! {};
    for i in 0..num_resources {
        let resource = resource(i);
        result = quote! {
            #result
            #resource,
        }
    }

    quote! {
        peregrine::initial_conditions! {
            #result
        }
    }
    .into()
}

#[proc_macro]
pub fn make_plan(input: TokenStream) -> TokenStream {
    let parser = Punctuated::<LitInt, Token![,]>::parse_terminated;
    let args = parser.parse(input).unwrap();
    let num_activities = args.get(0).unwrap().base10_parse::<usize>().unwrap();
    let num_activity_instances = args.get(1).unwrap().base10_parse::<usize>().unwrap();

    let mut result = quote! {};
    for i in 1..=num_activity_instances {
        let activity = activity(rand::random::<u64>() % num_activities as u64);
        let i = i as i64;
        let time = quote! {
            plan_start + #i.seconds() + #i.nanoseconds() + offset
        };
        result = quote! {
            #result
            plan.insert(#time, #activity)?;
        }
    }

    result.into()
}

#[proc_macro]
pub fn make_samples(input: TokenStream) -> TokenStream {
    let num_resources = parse_macro_input!(input as LitInt)
        .base10_parse::<usize>()
        .unwrap();

    let mut result = quote! {};
    for i in 0..num_resources {
        let ident = resource(i);
        let string = format!("{ident}: {{sample}}");
        result = quote! {
            #result
            {
                let start = Time::now()?;
                let sample = plan.sample::<#ident>(plan_start + 100.centuries())?;
                let end = Time::now()?;
                println!(#string);
                println!("time: {} s\n", (end - start).to_seconds());
            }
        };
    }
    result.into()
}
