#[plugy_macros::plugin]
pub trait Greeter {
    fn greet(&self, name: String, last_name: Option<String>) -> String;
}
