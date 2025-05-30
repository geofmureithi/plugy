//! # plugy-macro
//!
//! The `plugy-macro` crate provides a collection of macros that streamline the process of generating
//! bindings and interfaces for plugy's dynamic plugin system. These macros enhance the ergonomics of
//! working with plugins written in WebAssembly (Wasm) within your Rust applications.
//!
use convert_case::{Case, Casing};
use proc_macro::TokenStream;
use proc_macro2::{Ident, Span};
use quote::{quote, ToTokens};
use syn::{
    parse_macro_input, DeriveInput, FnArg, ImplItem, ImplItemFn, ItemImpl, ItemTrait, MetaNameValue,
};

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

    let mut generic_types = vec![];

    let async_methods = trait_methods.iter().map(|item| match item {
        syn::TraitItem::Fn(method) => {
            let method_name = &method.sig.ident;
            let method_inputs: Vec<_> = method
                .sig
                .inputs
                .iter()
                .map(|input| match &input {
                    FnArg::Receiver(_) => input.to_token_stream(),
                    FnArg::Typed(typed) => match *typed.ty.clone() {
                        syn::Type::Path(path) => {
                            if path.path.segments.iter().any(|seg| {
                                seg.ident == "Self"
                                    || seg.arguments.to_token_stream().to_string().contains("Self")
                            }) {
                                let arg_name = &typed.pat;
                                quote! {
                                    #arg_name: &Vec<u8>
                                }
                            } else {
                                input.to_token_stream()
                            }
                        }
                        _ => input.to_token_stream(),
                    },
                })
                .collect();
            let method_output = &method.sig.output;
            let method_name_str = method_name.to_string();
            let values: Vec<_> = method
                .sig
                .inputs
                .iter()
                .filter_map(|arg| match arg {
                    syn::FnArg::Receiver(_) => None,
                    syn::FnArg::Typed(t) => Some(t.pat.to_token_stream()),
                })
                .collect();
            quote! {
                pub async fn #method_name(#(#method_inputs), *) #method_output {
                    let func = self.handle.get_func(#method_name_str).await.unwrap();
                    func.call_unchecked(&(#(#values),*)).await
                }
            }
        }
        syn::TraitItem::Type(ty) => {
            let ident_type = &ty.ident;

            generic_types.push(quote! {
                #ident_type = Vec<u8>
            });
            quote! {}
        }

        _ => {
            quote! {}
        }
    });

    let callable_trait_name = format!("{}Wrapper", trait_name);
    let callable_trait_ident = syn::Ident::new(&callable_trait_name, trait_name.span());

    quote! {
        #[cfg(not(target_arch = "wasm32"))]
        #[derive(Debug, Clone)]
        pub struct #callable_trait_ident<P, D> {
            pub handle: plugy::runtime::PluginHandle<plugy::runtime::Plugin<D>>,
            inner: std::marker::PhantomData<P>
        }
        #[cfg(not(target_arch = "wasm32"))]
        impl<P, D: Clone + Send> #callable_trait_ident<P, D> {
            #(#async_methods)*
        }
        #[cfg(not(target_arch = "wasm32"))]
        impl<P, D> plugy::runtime::IntoCallable<P, D> for Box<dyn #trait_name<#(#generic_types),*>> {
            type Output = #callable_trait_ident<P, D>;
            fn into_callable(handle: plugy::runtime::PluginHandle<plugy::runtime::Plugin<D>>) -> Self::Output {
                #callable_trait_ident { handle, inner: std::marker::PhantomData }
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
                    let (value, #(#values),*): (#ty, #(#types),*)  = plugy::core::guest::read_msg(value);
                    plugy::core::guest::write_msg(&value.#method_name(#(#values),*))
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
            fn bytes(&self) -> std::pin::Pin<std::boxed::Box<dyn std::future::Future<Output = Result<Vec<u8>, anyhow::Error>>>> {
                std::boxed::Box::pin(async {
                    let res = std::fs::read(#file_path)?;
                    Ok(res)
                })
            }
            fn name(&self) -> &'static str {
                std::any::type_name::<Self>()
            }
        }
    }.into()
}

#[proc_macro_attribute]
pub fn context(args: TokenStream, input: TokenStream) -> TokenStream {
    // Parse the input as an ItemImpl
    let input = parse_macro_input!(input as ItemImpl);

    let data_ident = &args
        .into_iter()
        .nth(2)
        .map(|d| Ident::new(&d.to_string(), d.span().into()))
        .unwrap_or(Ident::new("_", Span::call_site()));

    // Get the name of the struct being implemented
    let struct_name = &input.self_ty.to_token_stream();

    let mod_name = Ident::new(
        &struct_name.to_string().to_case(Case::Snake),
        Span::call_site(),
    );

    let mut externs = Vec::new();

    let mut links = Vec::new();

    // Iterate over the items in the impl block to find methods
    let generated_methods = input
        .items
        .iter()
        .filter_map(|item| {
            if let syn::ImplItem::Fn(method) = item {
                let generics = &method.sig.generics;
                let method_name = &method.sig.ident;
                let method_args: Vec<_> = method
                    .sig
                    .inputs
                    .iter()
                    .skip(1) // Skip &caller
                    .map(|arg| {
                        if let FnArg::Typed(pat_type) = arg {
                            pat_type.to_token_stream()
                        } else {
                            panic!("Unsupported function argument type");
                        }
                    })
                    .collect();
                let method_pats: Vec<_> = method
                    .sig
                    .inputs
                    .iter()
                    .skip(1) // Skip &caller
                    .map(|arg| {
                        if let FnArg::Typed(pat_type) = arg {
                            pat_type.pat.to_token_stream()
                        } else {
                            panic!("Unsupported function argument type");
                        }
                    })
                    .collect();
                let return_type = &method.sig.output;
                let extern_method_name = Ident::new(
                    &format!("_plugy_context_{}", method_name),
                    Span::call_site(),
                );

                externs.push(quote::quote! {
                    extern "C" {
                        fn #extern_method_name(ptr: u64) -> u64;
                    }
                });

                let extern_method_name_str = extern_method_name.to_string();

                links.push(quote! {
                    linker
                        .func_wrap_async(
                            "env",
                            #extern_method_name_str,
                            move |mut caller: plugy::runtime::Caller<_>,
                                ptr: (u64,)|
                                -> Box<dyn std::future::Future<Output = u64> + Send> {
                                use plugy::core::bitwise::{from_bitwise, into_bitwise};
                                Box::new(async move {
                                    let store = caller.data().clone().unwrap();
                                    let plugy::runtime::RuntimeCaller {
                                        memory,
                                        alloc_fn,
                                        dealloc_fn,
                                        plugin
                                    } = store;

                                    let (ptr, len) = from_bitwise(ptr.0);
                                    let mut buffer = vec![0u8; len as _];
                                    memory.read(&mut caller, ptr as _, &mut buffer).unwrap();
                                    dealloc_fn
                                        .call_async(&mut caller, into_bitwise(ptr, len))
                                        .await
                                        .unwrap();
                                    let (#(#method_pats),*) = bincode::deserialize(&buffer).unwrap();
                                    let buffer =
                                        bincode::serialize(&#struct_name::#method_name(&mut caller, #(#method_pats),*).await)
                                            .unwrap();
                                    let ptr = alloc_fn
                                        .call_async(&mut caller, buffer.len() as _)
                                        .await
                                        .unwrap();
                                    memory.write(&mut caller, ptr as _, &buffer).unwrap();
                                    into_bitwise(ptr, buffer.len() as _)
                                })
                            },
                        )
                        .unwrap();
                });

                Some(quote! {
                    #[allow(unused_variables)]
                    pub fn #method_name #generics (#(#method_args),*) #return_type {
                        #[cfg(target_arch = "wasm32")]
                        {
                            let args = (#(#method_pats),*);
                            let ptr = plugy::core::guest::write_msg(&args);
                            unsafe { plugy::core::guest::read_msg(#extern_method_name(ptr)) }
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        panic!("You are trying to call wasm methods outside of wasm32")
                    }
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    // Generate the code for the context methods
    let generated = quote::quote! {
        #[cfg(not(target_arch = "wasm32"))]
        #input
        #[cfg(not(target_arch = "wasm32"))]
        impl plugy::runtime::Context<#data_ident> for #struct_name {
            fn link(&self, linker: &mut plugy::runtime::Linker<plugy::runtime::Plugin<#data_ident>>) {
                #(#links)*
            }
        }

        impl #struct_name {
            pub fn get(&self) -> #mod_name::sync::#struct_name {
                #mod_name::sync::#struct_name
            }
        }
        pub mod #mod_name {
            pub mod sync {
                #[cfg(target_arch = "wasm32")]
                #(#externs)*

                pub struct #struct_name;
                impl #struct_name {
                    #(#generated_methods)*
                }
        }
    }
    };

    // Return the generated code as a TokenStream
    generated.into()
}
