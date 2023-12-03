//! # plugy-runtime
//!
//! The `plugy-runtime` crate serves as the heart of Plugy's dynamic plugin system, enabling the runtime management
//! and execution of plugins written in WebAssembly (Wasm). It provides functionalities for loading, running,
//! and interacting with plugins seamlessly within your Rust applications.

use anyhow::Context as ErrorContext;
use async_lock::RwLock;
use bincode::Error;
use dashmap::DashMap;
use plugy_core::bitwise::{from_bitwise, into_bitwise};
use plugy_core::PluginLoader;
use serde::{de::DeserializeOwned, Serialize};
use std::fmt;
use std::{marker::PhantomData, sync::Arc};
use wasmtime::{Engine, Instance, Module, Store};

pub type CallerStore<D = Plugin> = Arc<RwLock<Store<Option<RuntimeCaller<D>>>>>;

pub type Caller<'a, D = Plugin> = wasmtime::Caller<'a, Option<RuntimeCaller<D>>>;

pub type Linker<D = Plugin> = wasmtime::Linker<Option<RuntimeCaller<D>>>;

/// A runtime environment for managing plugins and instances.
///
/// The `Runtime` struct provides a runtime environment for managing plugins
/// and their instances. It allows you to load, manage, and interact with plugins
/// written in WebAssembly (Wasm). The runtime maintains a collection of loaded modules,
/// instances, and associated data for efficient plugin management.
///
/// The generic parameter `P` represents the trait that your plugins must implement.
/// This trait defines the methods that can be called on the plugins using their instances.
///
/// # Example
///
/// ```rust
/// use plugy::runtime::Runtime;
///
/// trait Plugin {
///     fn greet(&self);
/// }
/// let runtime = Runtime::<Box<dyn Plugin>>::new();
/// // Load and manage plugins...
/// ```
pub struct Runtime<T, P = Plugin> {
    engine: Engine,
    linker: Linker<P>,
    modules: DashMap<&'static str, RuntimeModule<P>>,
    structure: PhantomData<T>,
}

pub trait IntoCallable<P, D> {
    type Output;
    fn into_callable(handle: PluginHandle<Plugin<D>>) -> Self::Output;
}

/// A concrete type that represents a wasm plugin and its state
#[derive(Debug, Clone)]
pub struct Plugin<D = Vec<u8>> {
    pub name: String,
    pub plugin_type: String,
    pub data: D,
}

impl Plugin {
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }

    pub fn plugin_type(&self) -> &str {
        self.plugin_type.as_ref()
    }

    pub fn data<T: DeserializeOwned>(&self) -> Result<T, Error> {
        bincode::deserialize(&self.data)
    }

    pub fn update<T: Serialize>(&mut self, value: &T) {
        self.data = bincode::serialize(value).unwrap()
    }
}

/// Single runnable module
#[allow(dead_code)]
pub struct RuntimeModule<P> {
    inner: Module,
    store: CallerStore<P>,
    instance: Instance,
}

/// The caller of a function
#[allow(dead_code)]
#[derive(Clone)]
pub struct RuntimeCaller<P> {
    pub memory: wasmtime::Memory,
    pub alloc_fn: wasmtime::TypedFunc<u32, u32>,
    pub dealloc_fn: wasmtime::TypedFunc<u64, ()>,
    pub plugin: P,
}

impl<P: std::fmt::Debug> fmt::Debug for RuntimeCaller<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RuntimeCaller")
            .field("memory", &self.memory)
            .field("alloc_fn", &"TypedFunc<u32, u32>")
            .field("dealloc_fn", &"TypedFunc<u64, ()>")
            .field("plugin", &self.plugin)
            .finish()
    }
}

impl<T, D: Send> Runtime<T, Plugin<D>> {
    /// Loads a plugin using the provided loader and returns the plugin instance.
    ///
    /// This asynchronous function loads a plugin by calling the `load` method on
    /// the provided `PluginLoader` instance. It then prepares the plugin for execution,
    /// instantiates it, and returns the plugin instance wrapped in the appropriate
    /// callable type.
    ///
    /// # Parameters
    ///
    /// - `loader`: An instance of a type that implements the `PluginLoader` trait,
    ///   responsible for loading the plugin's Wasm module data.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the loaded plugin instance on success,
    /// or an `anyhow::Error` if the loading and instantiation process encounters any issues.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use plugy_runtime::Runtime;
    /// use plugy_core::PluginLoader;
    /// use plugy_macros::*;
    /// use std::future::Future;
    /// use std::pin::Pin;
    /// #[plugy_macros::plugin]
    /// trait Plugin {
    ///     fn do_stuff(&self);
    /// }
    ///
    /// // impl Plugin for MyPlugin goes to the wasm file
    /// #[plugin_import(file = "target/wasm32-unknown-unknown/debug/my_plugin.wasm")]
    /// struct MyPlugin;
    ///
    /// async fn example(runtime: &Runtime<Box<dyn Plugin>>) -> anyhow::Result<()> {
    ///     let plugin = runtime.load(MyPlugin).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn load_with<P: Send + PluginLoader + Into<Plugin<D>>>(
        &self,
        plugin: P,
    ) -> anyhow::Result<T::Output>
    where
        T: IntoCallable<P, D>,
    {
        let bytes = plugin.bytes().await?;
        let name = plugin.name();
        let module = Module::new(&self.engine, bytes)?;
        let instance_pre = self.linker.instantiate_pre(&module)?;
        let mut store: Store<Option<RuntimeCaller<Plugin<D>>>> = Store::new(&self.engine, None);
        let instance = instance_pre.instantiate_async(&mut store).await?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .context("missing memory")?;
        let alloc_fn = instance.get_typed_func(&mut store, "alloc")?;
        let dealloc_fn = instance.get_typed_func(&mut store, "dealloc")?;
        *store.data_mut() = Some(RuntimeCaller {
            memory,
            alloc_fn,
            dealloc_fn,
            plugin: plugin.into(),
        });
        self.modules.insert(
            name,
            RuntimeModule {
                inner: module.clone(),
                store: Arc::new(RwLock::new(store)),
                instance,
            },
        );
        let plugin = self.get_plugin_by_name::<P>(&name)?;
        Ok(plugin)
    }

    /// Retrieves the callable plugin instance with the specified name.
    ///
    /// This function returns a callable instance of the loaded plugin with the
    /// specified name. The plugin must have been previously loaded using
    /// the `load` method or similar means.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the callable plugin instance on success,
    /// or an `anyhow::Error` if the instance retrieval encounters any issues.
    ///
    pub fn get_plugin_by_name<P: Send + PluginLoader>(
        &self,
        name: &str,
    ) -> anyhow::Result<T::Output>
    where
        T: IntoCallable<P, D>,
    {
        let module = self
            .modules
            .get(name)
            .context("missing plugin requested, did you forget .load")?;
        Ok(T::into_callable(PluginHandle {
            store: module.store.clone(),
            instance: module.instance,
        }))
    }

    /// Retrieves the callable plugin instance for the specified type.
    ///
    /// This function returns a callable instance of the loaded plugin for the
    /// specified type `T`. The plugin must have been previously loaded using
    /// the `load` method or similar means.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the callable plugin instance on success,
    /// or an `anyhow::Error` if the instance retrieval encounters any issues.
    ///
    pub fn get_plugin<P: Send + PluginLoader>(&self) -> anyhow::Result<T::Output>
    where
        T: IntoCallable<P, D>,
    {
        let name = std::any::type_name::<P>();
        let module = self
            .modules
            .get(name)
            .context("missing plugin requested, did you forget .load")?;
        Ok(T::into_callable(PluginHandle {
            store: module.store.clone(),
            instance: module.instance,
        }))
    }
}


impl<T> Runtime<T> {
    /// Loads a plugin using the provided loader and returns the plugin instance.
    ///
    /// This asynchronous function loads a plugin by calling the `load` method on
    /// the provided `PluginLoader` instance. It then prepares the plugin for execution,
    /// instantiates it, and returns the plugin instance wrapped in the appropriate
    /// callable type.
    ///
    /// # Parameters
    ///
    /// - `loader`: An instance of a type that implements the `PluginLoader` trait,
    ///   responsible for loading the plugin's Wasm module data.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the loaded plugin instance on success,
    /// or an `anyhow::Error` if the loading and instantiation process encounters any issues.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use plugy_runtime::Runtime;
    /// use plugy_core::PluginLoader;
    /// use plugy_macros::*;
    /// use std::future::Future;
    /// use std::pin::Pin;
    /// #[plugy_macros::plugin]
    /// trait Plugin {
    ///     fn do_stuff(&self);
    /// }
    ///
    /// // impl Plugin for MyPlugin goes to the wasm file
    /// #[plugin_import(file = "target/wasm32-unknown-unknown/debug/my_plugin.wasm")]
    /// struct MyPlugin;
    ///
    /// async fn example(runtime: &Runtime<Box<dyn Plugin>>) -> anyhow::Result<()> {
    ///     let plugin = runtime.load(MyPlugin).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn load<P: Send + PluginLoader + Into<Plugin>>(
        &self,
        plugin: P,
    ) -> anyhow::Result<T::Output>
    where
        T: IntoCallable<P, Vec<u8>>,
    {
        let bytes = plugin.bytes().await?;
        let name = plugin.name();
        let module = Module::new(&self.engine, bytes)?;
        let instance_pre = self.linker.instantiate_pre(&module)?;
        let mut store: Store<Option<RuntimeCaller<Plugin>>> = Store::new(&self.engine, None);
        let instance = instance_pre.instantiate_async(&mut store).await?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .context("missing memory")?;
        let alloc_fn = instance.get_typed_func(&mut store, "alloc")?;
        let dealloc_fn = instance.get_typed_func(&mut store, "dealloc")?;
        *store.data_mut() = Some(RuntimeCaller {
            memory,
            alloc_fn,
            dealloc_fn,
            plugin: plugin.into(),
        });
        self.modules.insert(
            name,
            RuntimeModule {
                inner: module.clone(),
                store: Arc::new(RwLock::new(store)),
                instance,
            },
        );
        let plugin = self.get_plugin_by_name::<P>(&name)?;
        Ok(plugin)
    }
}

impl<T, P> Runtime<T, P> {
    /// Creates a new instance of the `Runtime` with default configuration.
    ///
    /// This function initializes a `Runtime` instance using the default configuration
    /// settings for the underlying `wasmtime::Config`. It sets up the engine and linker,
    /// preparing it to load and manage plugin modules.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the initialized `Runtime` instance on success,
    /// or an `anyhow::Error` if the creation process encounters any issues.
    pub fn new() -> anyhow::Result<Self> {
        let mut config = wasmtime::Config::new();
        config.async_support(true);
        let engine = Engine::new(&config)?;
        let linker = Linker::new(&engine);
        let modules = DashMap::new();
        Ok(Self {
            engine,
            linker,
            modules,
            structure: PhantomData,
        })
    }
}

impl<T, D> Runtime<T, Plugin<D>> {
    /// Allows exposing methods that will run on the runtime side
    /// ```rust
    /// #[derive(Debug)]
    /// pub struct Logger;
    ///
    /// #[plugy::macros::context]
    /// impl Logger {
    ///     pub async fn log(_: &mut plugy::runtime::Caller<'_>, text: &str) {
    ///         dbg!(text);
    ///     }
    /// }
    /// fn main() {
    ///     let mut runtime = Runtime::<Box<dyn Greeter>>::new().unwrap();
    ///     let runtime = runtime
    ///         .context(Logger);
    /// }
    /// ````

    pub fn context<C: Context<D>>(&mut self, ctx: C) -> &mut Self {
        ctx.link(&mut self.linker);
        self
    }
}

/// A handle to a loaded plugin instance.
///
/// This struct represents a handle to a loaded plugin instance. It holds a reference
/// to the underlying instance, along with a reference to the associated store and
/// any additional data (`PhantomData<P>`) specific to the plugin type `P`.
///
/// # Type Parameters
///
/// - `P`: The plugin type that corresponds to this handle.
///
#[derive(Debug, Clone)]
pub struct PluginHandle<P = Plugin> {
    instance: Instance,
    store: CallerStore<P>,
}

impl<D> PluginHandle<Plugin<D>> {
    /// Retrieves a typed function interface from the loaded plugin instance.
    ///
    /// This method enables retrieving a typed function interface for a specific
    /// function name defined in the loaded plugin instance. The typed function
    /// interface provides a convenient way to invoke plugin functions and
    /// deserialize their input and output data.
    ///
    /// # Parameters
    ///
    /// - `name`: The name of the function in the plugin instance.
    ///
    /// # Type Parameters
    ///
    /// - `I`: The input data type expected by the function.
    /// - `R`: The output data type returned by the function.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the typed function interface on success,
    /// or an `anyhow::Error` if the function retrieval encounters any issues.

    pub async fn get_func<I: Serialize, R: DeserializeOwned>(
        &self,
        name: &str,
    ) -> anyhow::Result<Func<Plugin<D>, I, R>> {
        let store = self.store.clone();
        let inner_wasm_fn = self.instance.get_typed_func::<u64, u64>(
            &mut *store.write().await,
            &format!("_plugy_guest_{name}"),
        )?;
        Ok(Func {
            inner_wasm_fn,
            store,
            input: std::marker::PhantomData::<I>,
            output: std::marker::PhantomData::<R>,
        })
    }
}

pub struct Func<P, I: Serialize, R: DeserializeOwned> {
    inner_wasm_fn: wasmtime::TypedFunc<u64, u64>,
    store: CallerStore<P>,
    input: PhantomData<I>,
    output: PhantomData<R>,
}

impl<P: Send + Clone, R: DeserializeOwned, I: Serialize> Func<P, I, R> {
    /// Invokes the plugin function with the provided input, returning the result.
    ///
    /// This asynchronous method calls the plugin function using the provided input data
    /// without performing any error handling or result checking. If the function call
    /// fails, it will panic.
    ///
    /// # Parameters
    ///
    /// - `value`: The input data to be passed to the plugin function.
    ///
    /// # Returns
    ///
    /// Returns the result of the plugin function call.
    pub async fn call_unchecked(&self, value: &I) -> R {
        self.call_checked(value).await.unwrap()
    }
    /// Invokes the plugin function with the provided input, returning a checked result.
    ///
    /// This asynchronous method calls the plugin function using the provided input data
    /// and performs error handling to return a `Result` containing the result or any
    /// encountered errors.
    ///
    /// # Parameters
    ///
    /// - `value`: The input data to be passed to the plugin function.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the result of the plugin function call on success,
    /// or an `anyhow::Error` if the function call or deserialization encounters issues.

    pub async fn call_checked(&self, value: &I) -> anyhow::Result<R> {
        let mut store = self.store.write().await;
        let data = store.data_mut().clone().unwrap();
        let RuntimeCaller {
            memory, alloc_fn, ..
        } = data;

        let buffer = bincode::serialize(value)?;
        let len = buffer.len() as _;
        let ptr = alloc_fn.call_async(&mut *store, len).await?;
        memory.write(&mut *store, ptr as _, &buffer)?;
        let ptr = self
            .inner_wasm_fn
            .call_async(&mut *store, into_bitwise(ptr, len))
            .await?;
        let (ptr, len) = from_bitwise(ptr);
        let mut buffer = vec![0u8; len as _];
        memory.read(&mut *store, ptr as _, &mut buffer)?;
        Ok(bincode::deserialize(&buffer)?)
    }
}

pub trait Context<D = Vec<u8>>: Sized {
    fn link(&self, linker: &mut Linker<Plugin<D>>);
}
