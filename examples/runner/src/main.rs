use plugy::{core::PluginLoader, macros::plugin_import, runtime::Runtime};
use shared::{Fetcher, Greeter};

#[plugin_import(file = "target/wasm32-unknown-unknown/debug/foo_plugin.wasm")]
struct FooPlugin;

#[tokio::main]
async fn main() {
    let mut runtime = Runtime::<Box<dyn Greeter>, Fetcher>::new().unwrap();
    let handle = runtime.load_with(FooPlugin, |_| Fetcher).await.unwrap();
    let res = handle
        .greet("Geoff".to_owned(), Some("Mureithi".to_owned()))
        .await;

    assert_eq!(res, "Hello From Foo Plugin to Geoff Mureithi")
}
