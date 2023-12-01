use serde::{Serialize, Deserialize};

#[plugy::macros::plugin]
pub trait Greeter {
    fn greet(&self, name: String, last_name: Option<String>) -> String;
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Fetcher;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FooPluginConfig {
    config: String
}

#[plugy::macros::context]
impl Fetcher {
    pub async fn fetch(caller: &mut plugy::runtime::Caller<'_>, url: String) -> String {
        let data = &caller.data_mut().as_mut().unwrap().plugin;
        dbg!(data.data::<FooPluginConfig>().unwrap());
        let body = reqwest::get(url).await.unwrap().text().await.unwrap();
        body
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Logger;

#[plugy::macros::context]
impl Logger {
    pub async fn log(_: &mut plugy::runtime::Caller<'_>, text: &str) {
        dbg!(text);
    }
}
