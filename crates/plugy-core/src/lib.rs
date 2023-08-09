//! # plugy-core
//!
//! This crate contains fundamental components and utilities that serve as the building blocks for
//! plugy's dynamic plugin system. It provides essential functionalities that enable seamless integration
//! of plugins into your Rust applications using WebAssembly (Wasm).
//!
//! ## Modules
//!
//! - [`bitwise`](bitwise/index.html): A module providing utilities for working with bitwise operations and conversions.
//! - [`guest`](guest/index.html): A module that facilitates communication between the host application and Wasm plugins.
//!
pub mod bitwise;
pub mod guest;