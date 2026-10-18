//! Explicit provider-client opt-out from proxy discovery for isolated qualification.
//! This does not enforce offline execution: the caller must retain its OS sandbox.
use std::ffi::OsStr;

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

pub(crate) fn fallback_client() -> reqwest::Client {
    if enabled() {
        // Do not reintroduce system-proxy discovery via Client::new on a fallback.
        builder()
            .build()
            .expect("failed to build qualification provider HTTP client")
    } else {
        reqwest::Client::new()
    }
}
