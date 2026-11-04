#![allow(dead_code)]

// Can run independently of the application dependency graph:
// rustc --edition 2024 --test crates/jcode-tui/tests/keyboard_protocol.rs -o target/keyboard-protocol-tests.exe
// target/keyboard-protocol-tests.exe
mod logging {
    pub fn warn(message: &str) {
        eprintln!("{message}");
    }
}

#[path = "../src/tui/keyboard_protocol.rs"]
mod keyboard_protocol;
