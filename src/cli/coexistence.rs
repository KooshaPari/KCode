use anyhow::Result;
use serde::Serialize;
use std::ffi::OsStr;
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "coexistence/discovery.rs"]
mod discovery;
use discovery::{Discovery, discover};

impl Serialize for Discovery {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.label())
    }
}

#[derive(Serialize)]
struct SourceStatus {
    source: &'static str,
    host: &'static str,
    command_discovery: Discovery,
    authentication: &'static str,
    provider: &'static str,
    model: &'static str,
    sessions: &'static str,
}

#[derive(Serialize)]
struct Report {
    schema_version: u8,
    path_observed_at_unix_seconds: Option<u64>,
    sources: Vec<SourceStatus>,
    note: &'static str,
}
fn command_names(name: &'static str) -> Vec<String> {
    if cfg!(windows) {
        ["exe", "cmd", "bat"]
            .iter()
            .map(|ext| format!("{name}.{ext}"))
            .collect()
    } else {
        vec![name.to_string()]
    }
}

fn report(path: Option<&OsStr>, now: Option<u64>) -> Report {
    let sources = ["jcode", "forge", "codex"]
        .into_iter()
        .map(|source| {
            let names = command_names(source);
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            SourceStatus {
                source,
                host: "local",
                command_discovery: discover(path, &names),
                authentication: "unknown",
                provider: "unknown",
                model: "unknown",
                sessions: "unknown",
            }
        })
        .collect();
    Report {
        schema_version: 1,
        path_observed_at_unix_seconds: now,
        sources,
        note: "Checks bare command names on Unix and .exe, .cmd, or .bat on Windows. Other names are unobserved. A PATH file does not establish executable, installation, account, serving model, session access, or continuity. PATH entries may include links or remote filesystems.",
    }
}

fn observation_timestamp(now: SystemTime) -> Option<u64> {
    match now.duration_since(UNIX_EPOCH) {
        Ok(duration) => Some(duration.as_secs()),
        Err(error) => {
            eprintln!("Unable to timestamp local coexistence discovery: {error}");
            None
        }
    }
}

pub(super) fn run(json: bool) -> Result<()> {
    let now = observation_timestamp(SystemTime::now());
    let report = report(std::env::var_os("PATH").as_deref(), now);
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("Local coexistence discovery (continuity is unobserved)");
        for source in &report.sources {
            let state = match source.command_discovery {
                Discovery::FoundOnPath => "file found on PATH",
                Discovery::NotFoundOnPath => "no matching file found on PATH",
                Discovery::Unknown => "PATH discovery incomplete",
            };
            println!(
                "{}: {}; auth/provider/model/sessions: unknown",
                source.source, state
            );
        }
        println!("{}", report.note);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_before_epoch_is_unknown() {
        assert_eq!(
            observation_timestamp(UNIX_EPOCH - std::time::Duration::from_secs(1)),
            None
        );
        assert_eq!(
            observation_timestamp(UNIX_EPOCH + std::time::Duration::from_secs(7)),
            Some(7)
        );
    }

    #[test]
    fn reports_each_continuity_domain_unknown() {
        let report = report(None, Some(7));
        assert_eq!(report.sources.len(), 3);
        let serialized = serde_json::to_value(&report).unwrap();
        assert_eq!(serialized["schema_version"], 1);
        assert_eq!(serialized["path_observed_at_unix_seconds"], 7);
        assert_eq!(serialized["sources"][0]["source"], "jcode");
        assert_eq!(serialized["sources"][0]["command_discovery"], "unknown");
        for source in report.sources {
            assert_eq!(source.command_discovery, Discovery::Unknown);
            assert_eq!(
                [
                    source.authentication,
                    source.provider,
                    source.model,
                    source.sessions
                ],
                ["unknown"; 4]
            );
        }
    }
}
