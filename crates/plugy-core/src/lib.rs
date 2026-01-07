//! # plugy-core
//!
//! This crate contains fundamental components and utilities that serve as the building blocks for
//! plugy's dynamic plugin system. It provides essential functionalities that enable seamless integration
//! of plugins into your Rust applications using WebAssembly (Wasm).
//!
//!
//! ## Modules
//!
//! - [`bitwise`](bitwise/index.html): A module providing utilities for working with bitwise operations and conversions.
//! - [`guest`](guest/index.html): A module that facilitates communication between the host application and Wasm plugins.
//!

use std::{future::Future, pin::Pin};
pub mod bitwise;
pub mod guest;

/// A trait for loading plugin module data asynchronously.
///
/// This trait defines the behavior for loading plugin module data asynchronously.
/// Implementors of this trait provide the ability to asynchronously retrieve
/// the Wasm module data for a plugin.
///
pub trait PluginLoader {
    /// Asynchronously loads the Wasm module data for the plugin.
    ///
    /// This method returns a `Future` that produces a `Result` containing
    /// the Wasm module data as a `Vec<u8>` on success, or an `anyhow::Error`
    /// if loading encounters issues.
    ///
    /// # Returns
    ///
    /// Returns a `Pin<Box<dyn Future<Output = Result<Vec<u8>, anyhow::Error>>>>`
    /// representing the asynchronous loading process.
    fn bytes(&self) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, anyhow::Error>>>>;

    /// A plugins name should be known before loading.
    /// It might just be `std::any::type_name::<Self>()`
    fn name(&self) -> &'static str;
}
