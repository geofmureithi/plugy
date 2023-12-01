use plugy::{core::PluginLoader, macros::plugin_import, runtime::Runtime};
use serde::Serialize;
use shared::{Fetcher, Greeter, Logger};

#[plugin_import(file = "target/wasm32-unknown-unknown/debug/foo_plugin.wasm")]
#[derive(Debug, Serialize)]
struct FooPlugin {
    config: String,
}

#[tokio::main]
async fn main() {
    let mut runtime = Runtime::<Box<dyn Greeter>>::new().unwrap();
    let runtime = runtime
        // Include the fetcher context
        .context(Fetcher)
        // Include the logger context
        .context(Logger);
    let handle = runtime
        .load(FooPlugin {
            config: "Happy".to_string(),
        })
        .await
        .unwrap();
    let res = handle
        .greet("Geoff".to_owned(), Some("Mureithi".to_owned()))
        .await;
    println!("{res}");
    assert_eq!(res, "Hello From Foo Plugin to Geoff Mureithi")
}
