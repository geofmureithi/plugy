use plugy::{
    core::PluginLoader,
    macros::plugin_import,
    runtime::{Plugin, Runtime},
};
use shared::{Addr, Fetcher, Greeter, Logger, Printer};
use xtra::Mailbox;

#[plugin_import(file = "target/wasm32-unknown-unknown/debug/foo_plugin.wasm")]
#[derive(Debug)]
struct FooPlugin {
    addr: Addr,
}
impl From<FooPlugin> for Plugin<Addr> {
    fn from(val: FooPlugin) -> Self {
        Plugin {
            name: "FooPlugin".to_string(),
            data: val.addr,
            plugin_type: "FooPlugin".to_string(),
        }
    }
}

#[tokio::main]
async fn main() {
    let mut runtime = Runtime::<Box<dyn Greeter>, Plugin<Addr>>::new().unwrap();
    let runtime = runtime
        // Include the fetcher context
        .context(Fetcher)
        // Include the logger context
        .context(Logger);
    let handle = runtime
        .load_with(FooPlugin {
            addr: xtra::spawn_tokio(Printer::default(), Mailbox::unbounded()),
        })
        .await
        .unwrap();
    let res = handle
        .greet("Geoff".to_owned(), Some("Mureithi".to_owned()))
        .await;
    println!("{res}");
    assert_eq!(res, "Hello From Foo Plugin to Geoff Mureithi")
}
