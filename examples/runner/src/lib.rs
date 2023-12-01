use serde::{Serialize, Deserialize};

#[plugy::macros::plugin]
pub trait Greeter {
    fn greet(&self, name: String, last_name: Option<String>) -> String;
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Fetcher;

#[plugy::macros::context]
impl Fetcher {
    pub async fn fetch(&self, _: &plugy::runtime::Caller<'_, Self>, url: String) -> String {
        let body = reqwest::get(url).await.unwrap().text().await.unwrap();
        body
    }
}
