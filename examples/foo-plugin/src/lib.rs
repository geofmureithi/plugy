use plugy::macros::plugin_impl;
use shared::{fetcher::sync::Fetcher, logger::sync::Logger, Greeter};

#[derive(Debug)]
struct FooPlugin;

#[plugin_impl]
impl Greeter for FooPlugin {
    fn greet(&self, name: String, last_name: Option<String>) -> String {
        let res = Fetcher::fetch("https://github.com".to_owned());
        Logger::log(&res);
        let last_name = last_name.unwrap_or_default();

        format!("Hello From Foo Plugin to {name} {last_name}")
    }
}
