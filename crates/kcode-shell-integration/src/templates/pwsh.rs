//! PowerShell templates.

/// Precmd hook function.
pub const PWSH_PRECMD: &str = r#"function Invoke-KcodePreCmd {
    $exitCode = $global:LASTEXITCODE
    [Console]::Write("`e]133;D;${exitCode}`a")
    [Console]::Write("`e]133;A`a")
}
"#;

/// Preexec hook function.
pub const PWSH_PREEXEC: &str = r#"function Invoke-KcodePreExec {
    [Console]::Write("`e]133;B`a")
}
"#;

/// Full init script for PowerShell.
pub const PWSH_INIT: &str = r#"# kcode shell integration for PowerShell
# Auto-generated — do not edit manually.

function Invoke-KcodePreCmd {
    $exitCode = $global:LASTEXITCODE
    [Console]::Write("`e]133;D;${exitCode}`a")
    [Console]::Write("`e]133;A`a")
}

function Invoke-KcodePreExec {
    [Console]::Write("`e]133;B`a")
}

# Register hooks by wrapping the existing Prompt function
if (-not (Test-Path Variable:\KcodeOriginalPrompt)) {
    $global:KcodeOriginalPrompt = $function:Prompt
    $global:Prompt = {
        Invoke-KcodePreCmd
        & $global:KcodeOriginalPrompt
    }
}

# Report exit code on shell exit
Register-EngineEvent -SourceIdentifier PowerShell.Exiting -Action {
    [Console]::Write("`e]133;D;0`a")
}
"#;

/// Completion header.
pub const PWSH_COMPLETION_HEADER: &str = "# kcode PowerShell completions\n# Auto-generated — do not edit manually.\n\n";
