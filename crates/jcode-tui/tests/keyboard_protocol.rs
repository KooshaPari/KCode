// Can run independently of the application dependency graph:
// rustc --edition 2024 --test crates/jcode-tui/tests/keyboard_protocol.rs -o target/keyboard-protocol-tests.exe
// target/keyboard-protocol-tests.exe
#[path = "../src/tui/keyboard_protocol.rs"]
mod keyboard_protocol;
