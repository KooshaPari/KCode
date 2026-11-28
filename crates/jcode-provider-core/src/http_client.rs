//! Explicit provider-client opt-out from proxy discovery for isolated qualification.
//! This does not enforce offline execution: the caller must retain its OS sandbox.
use std::ffi::OsStr;
use std::time::Duration;

fn enabled() -> bool {
    std::env::var_os("KCODE_QUALIFICATION_NO_PROXY").as_deref() == Some(OsStr::new("1"))
}

pub(crate) fn builder() -> reqwest::ClientBuilder {
    let builder = reqwest::Client::builder();
    if enabled() {
        // Supported reqwest API: skip environment and macOS system proxy discovery.
        builder.no_proxy()
    } else {
        builder
    }
}

/// Constructs the last-resort provider HTTP client.
///
/// A build failure returns `reqwest::Client::new()` instead of panicking, so a
/// qualification run degrades to proxy discovery rather than aborting.
pub(crate) fn fallback_client() -> reqwest::Client {
    if enabled() {
        // Do not reintroduce system-proxy discovery via Client::new on a fallback.
        match builder().build() {
            Ok(client) => client,
            Err(_) => reqwest::Client::new(),
        }
    } else {
        reqwest::Client::new()
    }
}

/// Canonical User-Agent for generic outbound Jcode HTTP requests.
pub const KCODE_USER_AGENT: &str = concat!("jcode/", env!("CARGO_PKG_VERSION"));

/// Shared HTTP client for all generic provider requests. Creating a `reqwest::Client` is expensive
/// (~10ms due to TLS init, connection pool setup), so we reuse a single instance. Provider-specific
/// transports may override the User-Agent on individual requests when they intentionally need to
/// match an official client.
pub fn shared_http_client() -> reqwest::Client {
    use std::sync::OnceLock;
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            builder()
                .user_agent(KCODE_USER_AGENT)
                .connect_timeout(Duration::from_secs(15))
                .tcp_keepalive(Some(Duration::from_secs(30)))
                // Proactively detect half-dead pooled HTTP/2 connections before we
                // reuse them. Without keepalive pings, a stale multiplexed connection
                // (common behind NAT/VPN/proxy or flaky Wi-Fi) surfaces as
                // "http2 error: stream error received: unspecific protocol error".
                // Pinging while idle lets reqwest drop the connection instead.
                .http2_keep_alive_interval(Some(Duration::from_secs(30)))
                .http2_keep_alive_timeout(Duration::from_secs(15))
                .http2_keep_alive_while_idle(true)
                .pool_idle_timeout(Duration::from_secs(90))
                .pool_max_idle_per_host(8)
                .build()
                .unwrap_or_else(|err| {
                    eprintln!("jcode: failed to build shared provider HTTP client: {err}");
                    match builder()
                        .user_agent(KCODE_USER_AGENT)
                        .build()
                    {
                        Ok(client) => client,
                        Err(fallback_err) => {
                            eprintln!(
                                "jcode: failed to build fallback provider HTTP client: {fallback_err}"
                            );
                            fallback_client()
                        }
                    }
                })
        })
        .clone()
}

/// Fresh HTTP client for transport-fault retries.
///
/// Retrying on the shared pooled client can reuse *other* idle connections
/// established through the same broken network path (corrupting middlebox,
/// flaky NAT/VPN) that produced a TLS fault like `BadRecordMac` - so the
/// retry fails the same way. This client disables connection pooling, which
/// guarantees the retry opens a brand-new TCP+TLS connection (the property
/// that makes transport-fault retries actually succeed). Building a client
/// costs ~10ms, which is fine on a retry path that already backs off >=1s.
pub fn fresh_transport_client() -> reqwest::Client {
    builder()
        .user_agent(KCODE_USER_AGENT)
        .connect_timeout(Duration::from_secs(15))
        .tcp_keepalive(Some(Duration::from_secs(30)))
        .http2_keep_alive_interval(Some(Duration::from_secs(30)))
        .http2_keep_alive_timeout(Duration::from_secs(15))
        .http2_keep_alive_while_idle(true)
        // No pooled reuse: every request gets a fresh connection.
        .pool_max_idle_per_host(0)
        .build()
        .unwrap_or_else(|_| shared_http_client())
}
