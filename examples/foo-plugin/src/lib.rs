use plugy_macros::plugin_impl;
use shared::Greeter;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct FooPlugin;

#[plugin_impl]
impl Greeter for FooPlugin {
    fn greet(&self) -> String {
        "Hello From Foo Plugin".to_owned()
    }
}
