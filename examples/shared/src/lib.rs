#[plugy_macros::plugin]
pub trait Greeter {
    fn greet(&self) -> String;
}