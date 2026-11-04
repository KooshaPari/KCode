//! Client-side release-version ordering against a connected remote server.
//!
//! Extracted from `server_events.rs` so the version-comparison concern has its
//! own file. The file it came from is tracked by the oversized-file ratchet in
//! `scripts/code_size_budget.json`, and the fork-version parsing rules below
//! need to be readable and testable on their own.
//!
//! The signal this produces is the missing client-side staleness detection
//! behind issue #295: a server old enough to predate the self-reported
//! staleness machinery reports `server_has_update: None`, so it can never tell
//! us it is stale and the client happily attaches to it (then a `set_route`-
//! shaped request explodes against the ancient protocol). We detect that case
//! independently here.

/// Parse a version string into an orderable `(major, minor, patch)`, but only
/// for *clean release* builds.
///
/// Dev/dirty builds share a base semver and cannot be ordered against each
/// other or against releases (issue #277/#291: a self-dev / branched daemon
/// must never be force-downgraded just because its version string differs). So
/// we refuse to classify anything carrying a `-dev` or `dirty` marker as an
/// orderable version and return `None`, leaving such daemons to the existing
/// `server_has_update` (mtime-directional) path.
///
/// A KCode fork version carries a `-k<major>.<minor>.<patch>` suffix, e.g.
/// `v0.93.398-k0.1.0`. The suffix is fork identity, not part of the base
/// release semver, so it is stripped before parsing. Without this, the patch
/// component would be `398-k0`, which does not parse as a u32, and EVERY fork
/// release would come back as `None` - making fork builds permanently
/// unorderable and silently disabling stale-server detection for them. The
/// suffix must be strict: only `-k` followed by exactly three dot-separated
/// digit groups is accepted, so a malformed version is still rejected.
pub(crate) fn parse_release_semver(version: &str) -> Option<(u32, u32, u32)> {
    let lower = version.trim().to_ascii_lowercase();
    if lower.contains("-dev") || lower.contains("dirty") {
        return None;
    }
    // Take the leading token, e.g. "v0.17.0 (d741696f)" -> "0.17.0".
    let token = lower
        .split([' ', '(', ')', ','])
        .next()
        .unwrap_or(&lower)
        .trim();
    let token = token.strip_prefix('v').unwrap_or(token);
    // Strip a strict KCode fork-version suffix, e.g. "0.93.398-k0.1.0" ->
    // "0.93.398". Anything that is not exactly "-k<digits>.<digits>.<digits>"
    // is left in place and therefore fails to parse below.
    let base = if let Some((base, fork)) = token.split_once("-k") {
        let fork_parts: Vec<&str> = fork.split('.').collect();
        if fork_parts.len() != 3
            || fork_parts
                .iter()
                .any(|part| part.is_empty() || !part.bytes().all(|c| c.is_ascii_digit()))
        {
            return None;
        }
        base
    } else {
        token
    };
    let mut parts = base.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

/// True when the connected server reports a clean release version strictly
/// older than this client's own clean release version.
///
/// Gated on clean release semvers on BOTH sides, so dev/dirty/self-dev daemons
/// (which cannot be ordered) are never affected.
pub(crate) fn server_release_is_older_than_client(
    server_version: Option<&str>,
    client_version: &str,
) -> bool {
    let Some(server) = server_version.and_then(parse_release_semver) else {
        return false;
    };
    let Some(client) = parse_release_semver(client_version) else {
        return false;
    };
    server < client
}

#[cfg(test)]
mod tests {
    use super::{parse_release_semver, server_release_is_older_than_client};

    #[test]
    fn parse_release_semver_refuses_unorderable_dev_builds() {
        for value in [
            "0.0.1-dev",
            "0.0.1 (d741696f, dirty)",
            "v0.17.0-dev (abc1234)",
            "v0.93.398-k0.1.0-dev",
            "unknown",
        ] {
            assert_eq!(parse_release_semver(value), None, "{value}");
        }
    }

    #[test]
    fn parse_release_semver_reads_plain_release_versions() {
        assert_eq!(parse_release_semver("v0.17.0 (d741696f)"), Some((0, 17, 0)));
        assert_eq!(parse_release_semver("0.14.2"), Some((0, 14, 2)));
        assert_eq!(parse_release_semver("v1.2.3"), Some((1, 2, 3)));
    }

    #[test]
    fn parse_release_semver_strips_the_fork_version_suffix() {
        // The suffix is fork identity, not part of the base semver.
        assert_eq!(
            parse_release_semver("v0.93.398-k0.1.0"),
            Some((0, 93, 398))
        );
        assert_eq!(
            parse_release_semver("0.93.398-k0.1.0"),
            Some((0, 93, 398))
        );
        assert_eq!(
            parse_release_semver("v0.93.398-k0.1.0 (abc1234)"),
            Some((0, 93, 398))
        );
    }

    #[test]
    fn parse_release_semver_rejects_malformed_fork_suffixes() {
        for value in ["v0.93.398-kbad", "v0.93.398-k0.1", "v0.93.398-k0.1.x"] {
            assert_eq!(parse_release_semver(value), None, "{value}");
        }
    }

    #[test]
    fn server_release_older_than_client_detects_fork_stale_server() {
        // A fork server older than a fork client must be detected as stale.
        assert!(server_release_is_older_than_client(
            Some("v0.93.398-k0.1.0"),
            "v0.93.398-k0.2.0"
        ));
        assert!(!server_release_is_older_than_client(
            Some("v0.93.398-k0.2.0"),
            "v0.93.398-k0.1.0"
        ));
        assert!(!server_release_is_older_than_client(
            Some("v0.93.398-k0.1.0"),
            "v0.93.398-k0.1.0"
        ));
    }

    #[test]
    fn server_release_older_than_client_is_selfdev_safe() {
        // A dev server or a dev client is never orderable, so never "older".
        assert!(!server_release_is_older_than_client(
            Some("v0.93.398-k0.1.0-dev"),
            "v0.93.398-k0.1.0"
        ));
        assert!(!server_release_is_older_than_client(
            Some("v0.93.398-k0.1.0"),
            "v0.93.398-k0.1.0-dev"
        ));
        assert!(!server_release_is_older_than_client(None, "v0.93.398-k0.1.0"));
    }
}
