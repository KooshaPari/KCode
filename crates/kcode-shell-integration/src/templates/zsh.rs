//! Zsh shell templates.

/// Preexec hook (runs before each command).
pub const ZSH_PREEXEC: &str = r#"__kcode_preexec() {
    printf '\033]133;B\007'
    __KCODE_CMD_START=$(date +%s%N)
}
"#;

/// Precmd hook (runs before each prompt).
pub const ZSH_PRECMD: &str = r#"__kcode_precmd() {
    local exit_code=$?
    printf '\033]133;D;%d\007' $exit_code
    printf '\033]133;A\007'
    # Report CWD via OSC 7
    printf '\033]7;file://%s%s\007' ${(%)_:%%M} $PWD
}
"#;

/// Hook registration.
pub const ZSH_HOOK_REGISTER: &str = r#"autoload -Uz add-zsh-hook
add-zsh-hook preexec __kcode_preexec
add-zsh-hook precmd __kcode_precmd
"#;

/// Full init script for zsh.
pub const ZSH_INIT: &str = r#"# kcode shell integration for zsh
# Auto-generated — do not edit manually.

__kcode_preexec() {
    printf '\033]133;B\007'
    __KCODE_CMD_START=$(date +%s%N)
}

__kcode_precmd() {
    local exit_code=$?
    printf '\033]133;D;%d\007' $exit_code
    printf '\033]133;A\007'
    # Report CWD via OSC 7
    printf '\033]7;file://%s%s\007' ${(%)_:%%M} $PWD
}

autoload -Uz add-zsh-hook
add-zsh-hook preexec __kcode_preexec
add-zsh-hook precmd __kcode_precmd
"#;

/// Completion header.
pub const ZSH_COMPLETION_HEADER: &str = "# kcode zsh completions\n#compdef kcode\n# Auto-generated — do not edit manually.\n\n";
