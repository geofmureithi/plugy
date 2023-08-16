use plugy::macros::plugin_impl;
use serde::Deserialize;
use shared::{Greeter, Fetcher};

#[derive(Debug, Deserialize)]
struct FooPlugin;

#[plugin_impl]
impl Greeter for FooPlugin {
    fn greet(&self, name: String, last_name: Option<String>) -> String {
        let ctx = Fetcher::current();
        let _res = ctx.fetch("http://example.com".to_owned());
        let last_name = last_name.unwrap_or_default();
        format!("Hello From Foo Plugin to {name} {last_name}")
    }
}
