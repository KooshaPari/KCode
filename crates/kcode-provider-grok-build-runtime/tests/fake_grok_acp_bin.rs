// Minimal stub for the fake Grok ACP subprocess used by tests/fake_acp.rs.
// The real Grok CLI is not vendored; this stub reads stdin and exits 0 so
// the build can resolve `env!("CARGO_BIN_EXE_kcode-fake-grok-acp")`.
// The actual test behavior is exercised against the live Grok CLI in CI; here
// we only ensure the build artifact exists.

use std::io::Read;

fn main() {
    // Drain stdin so the parent process does not block on a full pipe.
    let mut buf = Vec::new();
    let _ = std::io::stdin().take(64 * 1024).read_to_end(&mut buf);
    // Write a minimal newline-terminated frame so callers parsing NDJSON do not panic.
    println!("{{\"type\":\"ready\"}}");
}