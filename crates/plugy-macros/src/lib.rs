use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, DeriveInput, ImplItem, ImplItemFn, ItemImpl, ItemTrait, MetaNameValue};

/// A procedural macro attribute for generating an asynchronous and callable version of a trait on the host side.
///
/// This procedural macro generates an asynchronous version of the provided trait by
/// wrapping its methods with async equivalents. It also generates a struct that
/// implements the asynchronous version of the trait and provides a way to call the
/// wrapped methods asynchronously.
///
/// # Arguments
///
/// This macro takes no arguments directly. It operates on the trait provided in the
/// input token stream.
///
/// # Examples
///
/// ```ignore
/// #[plugy_macros::plugin]
/// pub trait MyTrait {
///     fn sync_method(&self, param: u32) -> u32;
/// }
/// ```
#[proc_macro_attribute]
pub fn plugin(_: TokenStream, input: TokenStream) -> TokenStream {
    let original_trait = parse_macro_input!(input as ItemTrait);
    let async_trait = generate_async_trait(&original_trait);

    let output = quote! {
        #original_trait
        #async_trait
    };

    output.into()
}

fn generate_async_trait(trait_item: &ItemTrait) -> proc_macro2::TokenStream {
    let trait_name = &trait_item.ident;
    let trait_methods = &trait_item.items;

    let async_methods = trait_methods.iter().map(|item| {
        if let syn::TraitItem::Fn(method) = item {
            let method_name = &method.sig.ident;
            let method_inputs = &method.sig.inputs;
            let method_output = &method.sig.output;
            let method_name_str = method_name.to_string();
            let values: Vec<_> = method_inputs
                .iter()
                .filter_map(|arg| match arg {
                    syn::FnArg::Receiver(_) => None,
                    syn::FnArg::Typed(t) => Some(t.pat.to_token_stream()),
                })
                .collect();
            quote! {
                pub async fn #method_name(#method_inputs) #method_output {
                    let func = self.handle.get_func(#method_name_str).await.unwrap();
                    func.call_unchecked(&(#(#values),*)).await
                }
            }
        } else {
            item.to_token_stream()
        }
    });

    let callable_trait_name = format!("{}Wrapper", trait_name);
    let callable_trait_ident = syn::Ident::new(&callable_trait_name, trait_name.span());

    quote! {
        #[cfg(not(target_arch = "wasm32"))]
        #[derive(Debug, Clone)]
        pub struct #callable_trait_ident<P> {
            pub handle: plugy::runtime::PluginHandle<P>
        }
        #[cfg(not(target_arch = "wasm32"))]
        impl<P> #callable_trait_ident<P> {
            #(#async_methods)*
        }
        #[cfg(not(target_arch = "wasm32"))]
        impl<P> plugy::runtime::IntoCallable<P> for Box<dyn #trait_name> {
            type Output = #callable_trait_ident<P>;
            fn into_callable(handle: plugy::runtime::PluginHandle<P>) -> Self::Output {
                #callable_trait_ident { handle }
            }
        }
    }
}

fn impl_methods(imp: &ItemImpl) -> impl Iterator<Item = &ImplItemFn> {
    imp.items
        .iter()
        .filter_map(|i| match i {
            ImplItem::Fn(m) => Some(m),
            _ => None,
        })
        .filter(|_m| imp.trait_.is_some())
}

/// A procedural macro for generating guest-side implementations of trait methods.
///
/// This macro takes an implementation block for a trait and generates corresponding
/// guest-side functions for each method in the trait. The generated functions are
/// meant to be used in an external C interface for interacting with the methods from
/// a guest environment, such as WebAssembly.
///
/// The `plugin_impl` macro automates the process of generating unsafe external C
/// functions that can be called from a guest environment to invoke methods on the
/// trait implementation.
///
/// # Example
///
/// ```rust,ignore
/// use plugy_macros::plugin_impl;
///
/// trait Plugin {
///     fn greet(&self) -> String;
/// }
///
/// struct MyGreetPlugin;
///
/// #[plugin_impl]
/// impl Plugin for MyGreetPlugin {
///     fn greet(&self) -> String {
///         "Hello, from MyGreetPlugin!".to_string()
///     }
/// }
/// ```
///
/// In this example, the `plugin_impl` macro will generate bindings
/// the `greet` method from the `Plugin` trait. The generated function can then be
/// used to call the `greet` method from a host environment.
#[proc_macro_attribute]
pub fn plugin_impl(_metadata: TokenStream, input: TokenStream) -> TokenStream {
    let cur_impl: proc_macro2::TokenStream = input.clone().into();
    let imp = parse_macro_input!(input as ItemImpl);
    let ty = &imp.self_ty;
    let methods: Vec<&ImplItemFn> = impl_methods(&imp).collect();
    let derived: proc_macro2::TokenStream = methods
        .iter()
        .map(|m| {
            let method_name = &m.sig.ident;
            let args = &m.sig.inputs;
            let types: Vec<_> = args
                .iter()
                .filter_map(|arg| match arg {
                    syn::FnArg::Receiver(_) => None,
                    syn::FnArg::Typed(t) => Some(t.ty.to_token_stream()),
                })
                .collect();
            let values: Vec<_> = args
                .iter()
                .filter_map(|arg| match arg {
                    syn::FnArg::Receiver(_) => None,
                    syn::FnArg::Typed(t) => Some(t.pat.to_token_stream()),
                })
                .collect();
            let expose_name = format!("_plugy_guest_{}", method_name);
            let expose_name_ident = syn::Ident::new(&expose_name, Span::call_site());
            quote! {
                #[no_mangle]
                pub unsafe extern "C" fn #expose_name_ident(value: u64) -> u64 {
                    let (value, #(#values),*): (#ty, #(#types),*)  = plugy_core::guest::read_msg(value);
                    plugy_core::guest::write_msg(&value.#method_name(#(#values),*))
                }
            }
        })
        .collect();

    quote! {
        #cur_impl
        #derived
    }
    .into()
}

#[proc_macro_attribute]
pub fn plugin_import(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;
    let parsed = syn::parse2::<MetaNameValue>(args.into()).unwrap();
    assert_eq!(parsed.path.to_token_stream().to_string(), "file");
    let file_path = parsed.value;
    quote! {
        #input

        impl PluginLoader for #struct_name {
            fn load(&self) -> std::pin::Pin<std::boxed::Box<dyn std::future::Future<Output = Result<Vec<u8>, anyhow::Error>>>> {
                std::boxed::Box::pin(async {
                    let res = std::fs::read(#file_path)?;
                    Ok(res)
                })
            }
        }
    }.into()
}
