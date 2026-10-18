//! Zsh-specific dangerous command detection.
//!
//! Zsh builtins that can execute arbitrary code, redefine commands, load
//! native modules, or establish network connections. These are checked
//! against the base command (first word) of each command segment.
//!
//! Based on Claude Code's `ZSH_DANGEROUS_COMMANDS` set in `bashSecurity.ts`,
//! narrowed to the 17 general zsh built-ins that pose the most risk when
//! an AI agent can invoke arbitrary shell commands.

use std::collections::HashSet;
use std::sync::LazyLock;

/// The 17 zsh built-in commands considered dangerous.
pub static ZSH_DANGEROUS_COMMANDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut set = HashSet::with_capacity(17);
    for cmd in [
        "zmodload",
        "emulate",
        "sysopen",
        "ztcp",
        "exec",
        "eval",
        "source",
        "builtin",
        "setopt",
        "unsetopt",
        "alias",
        "unalias",
        "functions",
        "whence",
        "disable",
        "enable",
        "sched",
    ] {
        set.insert(cmd);
    }
    set
});

/// Returns `true` when `command` is a zsh dangerous built-in.
pub fn is_zsh_dangerous(command: &str) -> bool {
    ZSH_DANGEROUS_COMMANDS.contains(command)
}

/// Human-readable explanation of why `cmd` is dangerous.
///
/// Returns `None` for commands not in the dangerous set.
pub fn get_zsh_dangerous_reason(cmd: &str) -> Option<&'static str> {
    match cmd {
        "zmodload" => Some("Gateway to module loading attacks"),
        "emulate" => Some("eval-equivalent with -c flag"),
        "sysopen" => Some("File I/O bypassing shell safety"),
        "ztcp" => Some("TCP connections enable exfiltration"),
        "exec" => Some("Replaces shell process"),
        "eval" => Some("Arbitrary code execution"),
        "source" => Some("Execute arbitrary file"),
        "builtin" => Some("Bypass function redefinitions"),
        "setopt" | "unsetopt" => Some("Change shell behavior unsafely"),
        "alias" | "unalias" => Some("Redefine commands"),
        "functions" => Some("Define arbitrary functions"),
        "whence" => Some("Command path disclosure"),
        "disable" | "enable" => Some("Modify command availability"),
        "sched" => Some("Schedule arbitrary commands"),
        _ => None,
    }
}

#[cfg(test)]
#[path = "zsh_dangerous_tests.rs"]
mod zsh_dangerous_tests;
