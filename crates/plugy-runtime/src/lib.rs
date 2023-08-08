use dashmap::DashMap;
use plugy_core::bitwise::{from_bitwise, into_bitwise};
use serde::{de::DeserializeOwned, Serialize};
use std::fmt;
use std::{
    future::Future,
    marker::PhantomData,
    pin::Pin,
    sync::{Arc, Mutex},
};
use wasmtime::{Engine, Instance, Linker, Module, Store};
pub type Caller = Arc<Mutex<Store<Option<RuntimeCaller<()>>>>>;

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
///
/// fn main() {
///     let runtime = Runtime::<Box dyn Plugin>::new();
///
///     // Load and manage plugins...
/// }
pub struct Runtime<P> {
    engine: Engine,
    linker: Linker<Option<RuntimeCaller<()>>>,
    plugin_interface: PhantomData<P>,
    modules: DashMap<&'static str, RuntimeModule>,
}

/// Single runnable module
#[allow(dead_code)]
pub struct RuntimeModule {
    inner: Module,
    store: Arc<Mutex<Store<Option<RuntimeCaller<()>>>>>,
    instance: Instance,
}

/// The caller of a function
#[allow(dead_code)]
#[derive(Clone)]
pub struct RuntimeCaller<D> {
    memory: wasmtime::Memory,
    alloc_fn: wasmtime::TypedFunc<u32, u32>,
    dealloc_fn: wasmtime::TypedFunc<u64, ()>,
    data: D,
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

impl<P> Runtime<P> {
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
    /// use plugy_runtime::PluginLoader;
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
        P: IntoCallable<T>,
    {
        let bytes = loader.load().await?;
        let name = std::any::type_name::<T>();
        let module = Module::new(&self.engine, bytes)?;
        let instance_pre = self.linker.instantiate_pre(&module)?;
        let mut store = Store::new(&self.engine, None);
        let instance = instance_pre.instantiate_async(&mut store).await?;
        let memory = instance.get_memory(&mut store, "memory").unwrap();
        let alloc_fn = instance.get_typed_func(&mut store, "alloc")?;
        let dealloc_fn = instance.get_typed_func(&mut store, "dealloc")?;
        *store.data_mut() = Some(RuntimeCaller {
            memory,
            alloc_fn,
            dealloc_fn,
            data: (),
        });
        self.modules.insert(
            name,
            RuntimeModule {
                inner: module.clone(),
                store: Arc::new(Mutex::new(store)),
                instance,
            },
        );
        let plugin = self.get_plugin::<T>()?;
        Ok(plugin)
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
        P: IntoCallable<T>,
    {
        let name = std::any::type_name::<T>();
        let module = self.modules.get(name).unwrap();
        Ok(P::into_callable(PluginHandle {
            store: module.store.clone(),
            instance: module.instance.clone(),
            inner: PhantomData::<T>,
        }))
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
pub struct PluginHandle<P> {
    instance: Instance,
    store: Arc<Mutex<Store<Option<RuntimeCaller<()>>>>>,
    inner: PhantomData<P>,
}

impl<P> PluginHandle<P> {
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

    pub fn get_func<I: Serialize, R: DeserializeOwned>(
        &self,
        name: &str,
    ) -> anyhow::Result<Func<I, R>> {
        let store = self.store.clone();
        let inner_wasm_fn = self.instance.get_typed_func::<u64, u64>(
            &mut *store.lock().unwrap(),
            &format!("_plugy_guest_{name}"),
        )?;
        Ok(Func {
            inner_wasm_fn,
            store: store.clone(),
            input: std::marker::PhantomData::<I>,
            output: std::marker::PhantomData::<R>,
        })
    }
}

pub trait IntoCallable<P> {
    type Output;
    fn into_callable(handle: PluginHandle<P>) -> Self::Output;
}

pub struct Func<P: Serialize, R: DeserializeOwned> {
    inner_wasm_fn: wasmtime::TypedFunc<u64, u64>,
    store: Arc<Mutex<Store<Option<RuntimeCaller<()>>>>>,
    input: PhantomData<P>,
    output: PhantomData<R>,
}

impl<P: Serialize, R: DeserializeOwned> Func<P, R> {
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
        let mut store = self.store.lock().unwrap();
        let RuntimeCaller {
            memory, alloc_fn, ..
        } = store.data().clone().unwrap();

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

/// A trait for loading plugin module data asynchronously.
///
/// This trait defines the behavior for loading plugin module data asynchronously.
/// Implementors of this trait provide the ability to asynchronously retrieve
/// the Wasm module data for a plugin.
///
/// # Examples
///
/// ```rust
/// # use plugy::runtime::PluginLoader;
/// #
/// struct MyPluginLoader;
///
/// impl PluginLoader for MyPluginLoader {
///     fn load(&self) -> std::pin::Pin<std::boxed::Box<dyn std::future::Future<Output = Result<Vec<u8>, anyhow::Error>>>> {
///         // ... (implementation details)
///     }
/// }
/// ```
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
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use plugy::runtime::PluginLoader;
    /// #
    /// # struct MyPluginLoader;
    /// #
    /// # impl PluginLoader for MyPluginLoader {
    /// #     fn load(&self) -> std::pin::Pin<std::boxed::Box<dyn std::future::Future<Output = Result<Vec<u8>, anyhow::Error>>>> {
    /// #         Box::pin(async { Ok(Vec::new()) })
    /// #     }
    /// # }
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> anyhow::Result<()> {
    /// let loader = MyPluginLoader;
    /// let wasm_data: Vec<u8> = loader.load().await?;
    /// # Ok(())
    /// # }
    /// ```
    fn load(&self) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, anyhow::Error>>>>;
}
