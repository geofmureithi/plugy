use async_trait::async_trait;
#[cfg(not(target_arch = "wasm32"))]
use plugy::runtime::Plugin;
use serde::{Deserialize, Serialize};
use xtra::{Address, Handler};

#[plugy::macros::plugin]
pub trait Greeter {
    fn greet(&self, name: String, last_name: Option<String>) -> String;
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Fetcher;

#[plugy::macros::context(data = Addr)]
impl Fetcher {
    pub async fn fetch(_: &mut plugy::runtime::Caller<'_, Plugin<Addr>>, url: String) -> String {
        reqwest::get(url).await.unwrap().text().await.unwrap()
    }
}

pub type Addr = Address<Printer>;

#[derive(Default, xtra::Actor)]
pub struct Printer {
    pub times: usize,
}

struct Print(String);

#[async_trait]
impl Handler<Print> for Printer {
    type Return = ();

    async fn handle(&mut self, print: Print, _ctx: &mut xtra::Context<Self>) {
        self.times += 1;
        println!("Printing {}. Printed {} times so far.", print.0, self.times);
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Logger;

#[plugy::macros::context(data = Addr)]
impl Logger {
    pub async fn log(caller: &mut plugy::runtime::Caller<'_, Plugin<Addr>>, text: &str) {
        let plugin = &caller.data_mut().as_mut().unwrap().plugin;
        let addr = &plugin.data;
        addr.send(Print(text.to_string()))
            .await
            .expect("Printer should not be dropped");
    }
}
