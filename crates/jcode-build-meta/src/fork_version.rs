// Both build-time overrides and runtime wrappers must retain KCode's fork identity.
pub fn parse(value: &str) -> Option<String> {
    let value = value.trim().strip_prefix('v').unwrap_or(value.trim());
    let (base, fork) = value.split_once("-k")?;
    fn numeric(value: &str, nonzero: bool) -> Option<()> {
        let parts: Vec<_> = value.split('.').collect();
        if parts.len() != 3
            || parts.iter().any(|part| {
                part.is_empty()
                    || !part.bytes().all(|c| c.is_ascii_digit())
                    || (part.len() > 1 && part.starts_with('0'))
                    || part.parse::<u32>().is_err()
            })
            || (nonzero && parts.iter().all(|part| *part == "0"))
        {
            return None;
        }
        Some(())
    }
    numeric(base, true)?;
    let (fork_version, channel) = match fork.split_once('-') {
        Some((version, channel)) => (version, Some(channel)),
        None => (fork, None),
    };
    numeric(fork_version, false)?;
    if let Some(channel) = channel
        && channel
            .split(['.', '-'])
            .any(|part| part.is_empty() || !part.bytes().all(|c| c.is_ascii_alphanumeric()))
    {
        return None;
    }
    Some(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn fork_identity_preserves_valid_versions() {
        for value in [
            "0.93.0-k0.1.0",
            "1.2.3-k0.0.0",
            "1.2.3-k0.1.2-dev",
            "1.2.3-k0.1.2-alpha.1",
        ] {
            assert_eq!(parse(value).as_deref(), Some(value));
        }
        assert_eq!(parse(" v0.93.0-k0.1.0 ").as_deref(), Some("0.93.0-k0.1.0"));
    }

    #[test]
    fn fork_identity_rejects_stock_placeholder_and_malformed_versions() {
        for value in [
            "1.2.3",
            "0.0.0-k0.1.0",
            "1.2.3-",
            "1.2.3-kbad",
            "1.2.3-p0.1.0",
            "1.2.3-k0.1",
            "1.2.3-k0.1.0-",
            "1.2.3-k0.1.0- ",
            "1.2.3-k0.1.0-a..b",
            "vv1.2.3-k0.1.0",
            "01.2.3-k0.1.0",
            "1.2.3-k0.01.0",
        ] {
            assert_eq!(parse(value), None, "{value}");
        }
    }
}
