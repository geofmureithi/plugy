# How to run these examples.

There are three crates:

1. shared: (contains the plugin trait used by both runtime and plugins)
2. runner: the host part of the system.
3. foo-plugin: an example plugin implementation

To run:

- Compile the plugin (in foo-plugin):
  `cargo build --target wasm32-unknown-unknown`
- Run the host (in runner)
  `cargo run`
