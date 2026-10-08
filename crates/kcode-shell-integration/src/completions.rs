//! Shell-specific completion script generation for kcode commands.

use crate::detect::Shell;

/// A command that can be completed.
#[derive(Debug, Clone)]
pub struct CommandDef {
    pub name: String,
    pub description: String,
    pub subcommands: Vec<CommandDef>,
}

/// Generate completion script for the given shell and commands.
pub fn generate_completions(shell: Shell, commands: &[CommandDef]) -> String {
    match shell {
        Shell::Fish => fish_completions(commands),
        Shell::Zsh => zsh_completions(commands),
        Shell::Bash => bash_completions(commands),
        Shell::PowerShell => pwsh_completions(commands),
        Shell::Nushell => String::new(), // nushell uses its own completion system
        Shell::Elvish => String::new(),
        Shell::Unknown(_) => String::new(),
    }
}

fn fish_completions(commands: &[CommandDef]) -> String {
    let mut out = String::with_capacity(1024);
    out.push_str("# kcode completions for fish\n");
    out.push_str("# Install: kcode completions fish | source\n\n");

    for cmd in commands {
        out.push_str(&format!(
            "complete -c kcode -f -n '__fish_use_subcommand' -a '{}' -d '{}'\n",
            cmd.name, cmd.description,
        ));
        for sub in &cmd.subcommands {
            out.push_str(&format!(
                "complete -c kcode -f -n '__fish_seen_subcommand_from {}' -a '{}' -d '{}'\n",
                cmd.name, sub.name, sub.description,
            ));
        }
    }

    out
}

fn zsh_completions(commands: &[CommandDef]) -> String {
    let mut out = String::with_capacity(1024);
    out.push_str("#compdef kcode\n\n");
    out.push_str("# kcode completions for zsh\n");
    out.push_str("# Install: kcode completions zsh > ~/.zfunc/_kcode\n\n");

    out.push_str("_kcode() {\n");
    out.push_str("    local -a commands\n");
    out.push_str("    commands=(\n");

    for cmd in commands {
        out.push_str(&format!(
            "        '{}[{}]'\n",
            cmd.name, cmd.description,
        ));
    }

    out.push_str("    )\n\n");
    out.push_str("    _arguments -C \\\n");
    out.push_str("        '1:command:->command' \\\n");
    out.push_str("        '*::arg:->args'\n\n");
    out.push_str("    case $state in\n");
    out.push_str("        command)\n");
    out.push_str("            _describe 'command' commands\n");
    out.push_str("            ;;\n");

    for cmd in commands {
        if !cmd.subcommands.is_empty() {
            out.push_str(&format!("        {})\n", cmd.name));
            out.push_str("            _arguments \\\n");
            for sub in &cmd.subcommands {
                out.push_str(&format!(
                    "                '--{}[{}]'\n",
                    sub.name, sub.description,
                ));
            }
            out.push_str("            ;;\n");
        }
    }

    out.push_str("    esac\n");
    out.push_str("}\n\n");
    out.push_str("_kcode \"$@\"\n");

    out
}

fn bash_completions(commands: &[CommandDef]) -> String {
    let mut out = String::with_capacity(1024);
    out.push_str("# kcode completions for bash\n");
    out.push_str("# Install: kcode completions bash > /etc/bash_completion.d/kcode\n\n");

    out.push_str("_kcode() {\n");
    out.push_str("    local cur prev\n");
    out.push_str("    _init_completion || return\n\n");
    out.push_str("    case $cur in\n");

    for cmd in commands {
        out.push_str(&format!(
            "        {}*) COMPREPLY=(); return ;;\n",
            cmd.name,
        ));
    }

    out.push_str("    esac\n\n");
    out.push_str("    if [[ $COMP_CWORD -eq 1 ]]; then\n");
    out.push_str("        COMPREPLY=( $(compgen -W '");
    let names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
    out.push_str(&names.join(" "));
    out.push_str("' -- $cur) )\n");
    out.push_str("    fi\n");
    out.push_str("}\n\n");
    out.push_str("complete -F _kcode kcode\n");

    out
}

fn pwsh_completions(commands: &[CommandDef]) -> String {
    let mut out = String::with_capacity(1024);
    out.push_str("# kcode completions for PowerShell\n");
    out.push_str("# Install: kcode completions powershell | Invoke-Expression\n\n");

    out.push_str("Register-ArgumentCompleter -Native -CommandName 'kcode' -ScriptBlock {\n");
    out.push_str("    param($wordToComplete, $commandAst, $cursorPosition)\n\n");
    out.push_str("    $commands = @(\n");

    for cmd in commands {
        out.push_str(&format!(
            "        [PSCustomObject]@{{ Name = '{}'; Description = '{}' }}\n",
            cmd.name, cmd.description,
        ));
    }

    out.push_str("    )\n\n");
    out.push_str("    $commands | Where-Object { $_.Name -like \"$wordToComplete*\" } |\n");
    out.push_str("        ForEach-Object {\n");
    out.push_str("            [System.Management.Automation.CompletionResult]::new(\n");
    out.push_str("                $_.Name, $_.Name, 'ParameterValue', $_.Description\n");
    out.push_str("            )\n");
    out.push_str("        }\n");
    out.push_str("}\n");

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_commands() -> Vec<CommandDef> {
        vec![
            CommandDef {
                name: "ask".into(),
                description: "Ask a question".into(),
                subcommands: vec![],
            },
            CommandDef {
                name: "run".into(),
                description: "Run a command".into(),
                subcommands: vec![
                    CommandDef {
                        name: "--verbose".into(),
                        description: "Verbose output".into(),
                        subcommands: vec![],
                    },
                ],
            },
        ]
    }

    #[test]
    fn fish_completions_gen() {
        let cmds = test_commands();
        let out = generate_completions(Shell::Fish, &cmds);
        assert!(out.contains("complete -c kcode"));
        assert!(out.contains("-a 'ask'"));
        assert!(out.contains("-a 'run'"));
    }

    #[test]
    fn zsh_completions_gen() {
        let cmds = test_commands();
        let out = generate_completions(Shell::Zsh, &cmds);
        assert!(out.contains("#compdef kcode"));
        assert!(out.contains("_kcode()"));
        assert!(out.contains("'ask["));
    }

    #[test]
    fn bash_completions_gen() {
        let cmds = test_commands();
        let out = generate_completions(Shell::Bash, &cmds);
        assert!(out.contains("complete -F _kcode kcode"));
        assert!(out.contains("ask run"));
    }

    #[test]
    fn pwsh_completions_gen() {
        let cmds = test_commands();
        let out = generate_completions(Shell::PowerShell, &cmds);
        assert!(out.contains("Register-ArgumentCompleter"));
        assert!(out.contains("'ask'"));
    }

    #[test]
    fn empty_completions_for_unknown() {
        let out = generate_completions(Shell::Unknown("x".into()), &[]);
        assert!(out.is_empty());
    }
}
