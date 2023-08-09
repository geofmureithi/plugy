use plugy::{
    macros::plugin_import,
    runtime::{PluginLoader, Runtime},
};
use shared::Greeter;

#[plugin_import(file = "target/wasm32-unknown-unknown/debug/foo_plugin.wasm")]
struct FooPlugin;

#[tokio::main]
async fn main() {
    let runtime = Runtime::<Box<dyn Greeter>>::new().unwrap();
    let handle = runtime.load(FooPlugin).await.unwrap();
    let res = handle.greet("Geoff".to_owned(), Some("Mureithi".to_owned())).await;

    assert_eq!(res, "Hello From Foo Plugin to Geoff Mureithi")
}
