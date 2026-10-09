//! Bash shell templates.

/// Preexec hook (via DEBUG trap).
pub const BASH_PREEXEC: &str = r#"__kcode_preexec() {
    printf '\033]133;B\007'
    __KCODE_CMD_START=$(date +%s%N)
}
"#;

/// Precmd hook (PROMPT_COMMAND).
pub const BASH_PRECMD: &str = r#"__kcode_precmd() {
    local exit_code=$?
    printf '\033]133;D;%d\007' $exit_code
    printf '\033]133;A\007'
}
"#;

/// Full init script for bash.
pub const BASH_INIT: &str = r#"# kcode shell integration for bash
# Auto-generated — do not edit manually.

__kcode_preexec() {
    printf '\033]133;B\007'
    __KCODE_CMD_START=$(date +%s%N)
}

__kcode_precmd() {
    local exit_code=$?
    printf '\033]133;D;%d\007' $exit_code
    printf '\033]133;A\007'
}

# For bash >= 5.1, use DEBUG trap; older versions need PROMPT_COMMAND trick
if [[ ${BASH_VERSINFO[0]} -ge 5 ]] || [[ ${BASH_VERSINFO[0]} -eq 5 && ${BASH_VERSINFO[1]} -ge 1 ]]; then
    trap '__kcode_preexec' DEBUG
else
    # Bash < 5.1: use DEBUG trap for preexec
    trap '__kcode_preexec' DEBUG
fi

PROMPT_COMMAND=__kcode_precmd
"#;

/// Completion header.
pub const BASH_COMPLETION_HEADER: &str = "# kcode bash completions\n# Auto-generated — do not edit manually.\n\n";
