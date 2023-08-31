//! # plugy-runtime
//!
//! The `plugy-runtime` crate serves as the heart of Plugy's dynamic plugin system, enabling the runtime management
//! and execution of plugins written in WebAssembly (Wasm). It provides functionalities for loading, running,
//! and interacting with plugins seamlessly within your Rust applications.
use anyhow::Context as ErrorContext;
use async_lock::RwLock;
use dashmap::DashMap;
use plugy_core::bitwise::{from_bitwise, into_bitwise};
use plugy_core::PluginLoader;
use serde::{de::DeserializeOwned, Serialize};
use std::fmt;
use std::{marker::PhantomData, sync::Arc};
use wasmtime::{Engine, Instance, Module, Store};

pub type CallerStore<D = ()> = Arc<RwLock<Store<Option<RuntimeCaller<D>>>>>;

pub type Caller<'a, D> = wasmtime::Caller<'a, Option<RuntimeCaller<D>>>;

pub type Linker<D = ()> = wasmtime::Linker<Option<RuntimeCaller<D>>>;

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
pub struct Runtime<P, D = ()> {
    engine: Engine,
    linker: Linker<D>,
    plugin_interface: PhantomData<P>,
    modules: DashMap<&'static str, RuntimeModule<D>>,
}

/// Single runnable module
#[allow(dead_code)]
pub struct RuntimeModule<D> {
    inner: Module,
    store: CallerStore<D>,
    instance: Instance,
}

/// The caller of a function
#[allow(dead_code)]
#[derive(Clone)]
pub struct RuntimeCaller<D> {
    pub memory: wasmtime::Memory,
    pub alloc_fn: wasmtime::TypedFunc<u32, u32>,
    pub dealloc_fn: wasmtime::TypedFunc<u64, ()>,
    pub data: D,
}

impl<D: std::fmt::Debug> fmt::Debug for RuntimeCaller<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RuntimeCaller")
            .field("memory", &self.memory)
            .field("alloc_fn", &"TypedFunc<u32, u32>")
            .field("dealloc_fn", &"TypedFunc<u64, ()>")
            .field("data", &self.data)
            .finish()
    }
}

impl<P, D: Default + Send> Runtime<P, D> {
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
    pub async fn load<T: PluginLoader>(&self, loader: T) -> anyhow::Result<P::Output>
    where
        P: IntoCallable<P, D>,
    {
        let bytes = loader.load().await?;
        let name = loader.name();
        let module = Module::new(&self.engine, bytes)?;
        let instance_pre = self.linker.instantiate_pre(&module)?;
        let mut store: Store<Option<RuntimeCaller<D>>> = Store::new(&self.engine, None);
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
            data: D::default(),
        });
        self.modules.insert(
            name,
            RuntimeModule {
                inner: module.clone(),
                store: Arc::new(RwLock::new(store)),
                instance,
            },
        );
        let plugin = self.get_plugin_by_name(&name)?;
        Ok(plugin)
    }
}

impl<P, D: Send> Runtime<P, D> {
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
            plugin_interface: PhantomData,
        })
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
    pub fn get_plugin<T>(&self) -> anyhow::Result<P::Output>
    where
        P: IntoCallable<T, D>,
    {
        let name = std::any::type_name::<T>();
        let module = self
            .modules
            .get(name)
            .context("missing plugin requested, did you forget .load")?;
        Ok(P::into_callable(PluginHandle {
            store: module.store.clone(),
            instance: module.instance,
            inner: PhantomData::<T>,
        }))
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
    pub fn get_plugin_by_name(&self, name: &str) -> anyhow::Result<P::Output>
    where
        P: IntoCallable<P, D>,
    {
        let module = self
            .modules
            .get(name)
            .context("missing plugin requested, did you forget .load")?;
        Ok(P::into_callable(PluginHandle {
            store: module.store.clone(),
            instance: module.instance,
            inner: PhantomData::<P>,
        }))
    }

    /// Loads a plugin using the provided loader, but can customize the data stored and returns the plugin instance.
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
    /// - `data_fn`: A function that takes in the module and returns data to be used in that modules functions
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
    /// use plugy_runtime::Context;
    /// use plugy_runtime::Linker;
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
    /// struct Addr {
    ///     //eg actix or xtra
    /// }
    ///
    /// impl Context for Addr {
    ///     fn link(&self, linker: &mut Linker<Self>) {
    ///         //expose methods here
    ///     }
    /// }
    ///
    /// async fn example(runtime: &mut Runtime<Box<dyn Plugin>, Addr>) -> anyhow::Result<()> {
    ///     let plugin = runtime.load_with(MyPlugin, |_plugin| Addr {}).await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn load_with<T: PluginLoader>(
        &mut self,
        loader: T,
        data_fn: impl Fn(&T) -> D,
    ) -> anyhow::Result<P::Output>
    where
        P: IntoCallable<P, D>,
        D: Context,
    {
        let data = data_fn(&loader);
        data.link(&mut self.linker);
        let bytes = loader.load().await?;
        let name = loader.name();
        let module = Module::new(&self.engine, bytes)?;
        let instance_pre = self.linker.instantiate_pre(&module)?;
        let mut store: Store<Option<RuntimeCaller<D>>> = Store::new(&self.engine, None);
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
            data,
        });
        self.modules.insert(
            name,
            RuntimeModule {
                inner: module.clone(),
                store: Arc::new(RwLock::new(store)),
                instance,
            },
        );

        let plugin = self.get_plugin_by_name(&name)?;
        Ok(plugin)
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
pub struct PluginHandle<P, D> {
    instance: Instance,
    store: CallerStore<D>,
    inner: PhantomData<P>,
}

impl<P, D> PluginHandle<P, D> {
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
    ) -> anyhow::Result<Func<I, R, D>> {
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

pub trait IntoCallable<P, D> {
    type Output;
    fn into_callable(handle: PluginHandle<P, D>) -> Self::Output;
}

pub struct Func<P: Serialize, R: DeserializeOwned, D> {
    inner_wasm_fn: wasmtime::TypedFunc<u64, u64>,
    store: CallerStore<D>,
    input: PhantomData<P>,
    output: PhantomData<R>,
}

impl<P: Serialize, R: DeserializeOwned, D: Send + Clone> Func<P, R, D> {
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
    pub async fn call_unchecked(&self, value: &P) -> R {
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

    pub async fn call_checked(&self, value: &P) -> anyhow::Result<R> {
        let mut store = self.store.write().await;
        let RuntimeCaller {
            memory, alloc_fn, ..
        } = store.data().clone().context("missing data in store")?;

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

pub trait Context: Sized {
    fn link(&self, linker: &mut Linker<Self>);
}
