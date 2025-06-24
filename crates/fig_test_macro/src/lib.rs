use proc_macro::TokenStream;
use quote::quote;
use syn::{
    ItemFn,
    parse_macro_input,
};

#[proc_macro_attribute]
pub fn test(_: TokenStream, stream: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(stream as ItemFn);

    input.block.stmts.insert(
        0,
        syn::parse(quote!(let mut environment = fig_test::ENVIRONMENT_LOCK.blocking_lock();).into()).unwrap(),
    );
    input.block.stmts.insert(
        1,
        syn::parse(
            quote!({
                if let None = *environment {
                    *environment = Some(std::env::vars().collect());
                }

                for (key, value) in std::env::vars() {
                    std::env::remove_var(key);
                }

                for (key, value) in environment.as_ref().unwrap() {
                    std::env::set_var(key, value);
                }
            })
            .into(),
        )
        .unwrap(),
    );

    let expanded = quote! {
        #[test]
        #input
    };

    TokenStream::from(expanded)
}

#[proc_macro_attribute]
pub fn test_async(_: TokenStream, stream: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(stream as ItemFn);

    input.block.stmts.insert(
        0,
        syn::parse(quote!(let mut environment = fig_test::ENVIRONMENT_LOCK.lock().await;).into()).unwrap(),
    );
    input.block.stmts.insert(
        1,
        syn::parse(
            quote!({
                if let None = *environment {
                    *environment = Some(std::env::vars().collect());
                }

                for (key, value) in std::env::vars() {
                    std::env::remove_var(key);
                }

                for (key, value) in environment.as_ref().unwrap() {
                    std::env::set_var(key, value);
                }
            })
            .into(),
        )
        .unwrap(),
    );

    let expanded = quote! {
        #[tokio::test]
        #input
    };

    TokenStream::from(expanded)
}
