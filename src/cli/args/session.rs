use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub(crate) struct ImportJcodeArgs {
    /// Session ID in ~/.jcode/sessions (no paths or credentials imported)
    pub(crate) id: String,
    /// Preview the selected files and conflicts without importing
    #[arg(long)]
    pub(crate) preview: bool,
}

impl super::super::commands::session_commands::SessionImportRequest for ImportJcodeArgs {
    fn id(&self) -> &str {
        &self.id
    }
    fn preview(&self) -> bool {
        self.preview
    }
}

#[derive(Subcommand, Debug)]
pub(crate) enum SessionCommand {
    /// Copy a selected jcode session into KCode; never modifies the source
    ImportJcode {
        #[command(flatten)]
        session: ImportJcodeArgs,
    },
    /// Rename a saved session's human-readable name/title
    Rename {
        /// Session ID or memorable short name, e.g. fox
        session: String,

        /// New session name/title
        #[arg(required_unless_present = "clear")]
        name: Option<String>,

        /// Clear the custom session name/title
        #[arg(long, conflicts_with = "name")]
        clear: bool,

        /// Emit JSON instead of human-readable output
        #[arg(long)]
        json: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        command: SessionCommand,
    }

    #[test]
    fn import_preview_is_opt_in() {
        for (args, expected) in [
            (vec!["test", "import-jcode", "fox"], false),
            (vec!["test", "import-jcode", "fox", "--preview"], true),
        ] {
            let SessionCommand::ImportJcode { session } =
                TestCli::try_parse_from(args).unwrap().command
            else {
                panic!("expected import command");
            };
            assert_eq!(session.id, "fox");
            assert_eq!(session.preview, expected);
        }
    }
}
