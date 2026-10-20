# How to run these examples.

There are two crates:

1. runner: the host part of the system.
2. foo-plugin: an example plugin implementation

The runner crate has a `lib.rs` that can be reused by plugins

To run:

- Compile the plugin (in foo-plugin):
  `cargo build --target wasm32-unknown-unknown`
- Run the host (in runner)
  `cargo run`
