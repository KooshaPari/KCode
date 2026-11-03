//! Version ordering helpers for remote server events.
//!
//! Extracted from `server_events.rs` to keep that file under the code-size ratchet.

/// Parse a jcode version string into an orderable `(major, minor, patch)`, but
/// only for *clean release* builds.
///
/// Dev/dirty builds share a base semver and cannot be ordered against each other
/// or against releases (issue #277/#291: a self-dev / branched daemon must never
/// be force-downgraded just because its version string differs). So we refuse to
/// classify anything carrying a `-dev` or `dirty` marker as an orderable version
/// and return `None`, leaving such daemons to the existing `server_has_update`
/// (mtime-directional) path.
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
    // Strip a strict KCode fork-version suffix of the form "-k<major>.<minor>.<patch>"
    // (e.g. "v0.93.398-k0.1.0" -> "0.93.398"). The fork suffix is NOT part of the
    // base release semver and must not be parsed into the patch number. Only the
    // strict "-k<digits>.<digits>.<digits>" form is accepted; any other suffix is
    // left in place (and therefore fails to parse below).
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

/// True when the connected server reports a clean release version strictly older
/// than this client's own clean release version.
///
/// This is the missing client-side staleness signal behind issue #295: a server
/// old enough to predate the self-reported staleness machinery reports
/// `server_has_update: None`, so it can never tell us it is stale and the client
/// happily attaches to it (then a `set_route`-shaped request explodes against the
/// ancient protocol). We detect that case independently here.
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
        assert_eq!(parse_release_semver("v0.17.0 (d741696f)"), Some((0, 17, 0)));
        assert_eq!(parse_release_semver("0.14.2"), Some((0, 14, 2)));
        // Dev/dirty builds share a base semver and must not be ordered.
        assert_eq!(parse_release_semver("v0.18.4-dev (102e9750, dirty)"), None);
        assert_eq!(parse_release_semver("v0.14.2-dev (38452185, dirty)"), None);
        assert_eq!(parse_release_semver("unknown"), None);
        // KCode fork release versions carry a "-k<major>.<minor>.<patch>" suffix
        // that must not be absorbed into the base patch number (issue #295).
        assert_eq!(parse_release_semver("v0.93.398-k0.1.0"), Some((0, 93, 398)));
        assert_eq!(parse_release_semver("0.93.398-k0.1.0"), Some((0, 93, 398)));
        assert_eq!(
            parse_release_semver("v0.93.398-k0.1.0 (abc1234)"),
            Some((0, 93, 398))
        );
        // A dev fork build is still unorderable.
        assert_eq!(parse_release_semver("v0.93.398-k0.1.0-dev"), None);
        // Arbitrary (non-strict) suffixes are not accepted.
        assert_eq!(parse_release_semver("v0.93.398-kraken"), None);
        assert_eq!(parse_release_semver("v0.93.398-k0.1"), None);
    }

    #[test]
    fn server_release_older_than_client_detects_fork_stale_server() {
        // A fork server older than a newer fork client is detected as stale
        // (issue #295): previously parse_release_semver returned None for every
        // fork release, so this never fired.
        assert!(server_release_is_older_than_client(
            Some("v0.93.398-k0.1.0"),
            "v0.93.399-k0.1.0"
        ));
        assert!(!server_release_is_older_than_client(
            Some("v0.93.398-k0.1.0"),
            "v0.93.398-k0.1.0"
        ));
    }

    #[test]
    fn server_release_older_than_client_is_selfdev_safe() {
        // Clean release older than clean client -> stale.
        assert!(server_release_is_older_than_client(
            Some("v0.14.2 (38452185)"),
            "v0.17.0 (d741696f)"
        ));
        // Equal or newer -> not stale.
        assert!(!server_release_is_older_than_client(
            Some("v0.17.0"),
            "v0.17.0"
        ));
        assert!(!server_release_is_older_than_client(
            Some("v0.18.0"),
            "v0.17.0"
        ));
        // Either side dev/dirty/unparseable -> never claim staleness (protects
        // self-dev and branched daemons from a forced downgrade).
        assert!(!server_release_is_older_than_client(
            Some("v0.14.2-dev (abc, dirty)"),
            "v0.17.0"
        ));
        assert!(!server_release_is_older_than_client(
            Some("v0.14.2"),
            "v0.17.0-dev (abc, dirty)"
        ));
        assert!(!server_release_is_older_than_client(None, "v0.17.0"));
    }
}
