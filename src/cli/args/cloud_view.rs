//! Cloud session view format shared by `jcode cloud sessions view`.

use clap::ValueEnum;

#[derive(ValueEnum, Debug, Clone, Copy)]
pub(crate) enum CloudSessionViewFormat {
    Summary,
    Json,
    Html,
}

impl CloudSessionViewFormat {
    pub(crate) fn as_arg(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Json => "json",
            Self::Html => "html",
        }
    }
}
