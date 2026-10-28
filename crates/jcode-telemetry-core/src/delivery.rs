//! Telemetry delivery: HTTP posting, retry policy, and background workers.
//!
//! Extracted from `lib.rs` to keep that file within its tracked size. Every
//! failure path degrades to dropping the payload rather than panicking: a
//! host must never crash because an endpoint was unreachable.

// `telemetry_status_is_permanent`, `spawn_background_worker`, and
// `payload_is_breaker_exempt` are the only items the test module reaches for, so
// the live delivery surface below — and every import it alone needs — is compiled
// out of test builds instead of tripping `dead_code`/`unused_imports`.
#[cfg(not(test))]
use jcode_logging as logging;
#[cfg(not(test))]
use std::sync::OnceLock;
#[cfg(not(test))]
use std::sync::atomic::Ordering;
#[cfg(not(test))]
use std::sync::mpsc::TrySendError;
use std::sync::mpsc::{SyncSender, sync_channel};
#[cfg(not(test))]
use std::time::Duration;

use serde_json::Value;

#[cfg(not(test))]
use super::{DeliveryMode, TELEMETRY_ENDPOINT, TRANSCRIPT_ENDPOINT};

/// Delivery state shared between this module and the parent's `send_payload`.
///
/// It lives here, next to the rest of the delivery surface, so `lib.rs` does
/// not have to carry the queue and overflow flags.
#[cfg(not(test))]
pub(crate) mod delivery_state {
    use super::SyncSender;
    use std::sync::OnceLock;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    /// Queue capacity for the background telemetry channel. Payloads are
    /// dropped rather than blocking the caller once it is full.
    pub(crate) const BACKGROUND_QUEUE_CAPACITY: usize = 2048;
    /// Timeout applied to a single background delivery attempt.
    pub(crate) const ASYNC_SEND_TIMEOUT: Duration = Duration::from_secs(5);
    /// Set once the endpoint has rejected telemetry permanently, so later
    /// payloads are dropped without another request.
    pub(crate) static TELEMETRY_PERMANENTLY_REJECTED: AtomicBool = AtomicBool::new(false);
    /// Guards a single warning per overflow episode; reset once a payload lands.
    pub(crate) static TELEMETRY_QUEUE_OVERFLOW_WARNED: AtomicBool = AtomicBool::new(false);
    pub(crate) static TELEMETRY_BACKGROUND_SENDER: OnceLock<Option<SyncSender<serde_json::Value>>> =
        OnceLock::new();
    pub(crate) static TRANSCRIPT_BACKGROUND_SENDER: OnceLock<
        Option<SyncSender<serde_json::Value>>,
    > = OnceLock::new();
    pub(crate) static TELEMETRY_HTTP_CLIENT: OnceLock<Option<reqwest::blocking::Client>> =
        OnceLock::new();
}

/// Lazily initialize and borrow the shared telemetry HTTP client. Returns `None`
/// when the builder fails, keeping telemetry optional rather than crashing.
#[cfg(not(test))]
pub(crate) fn http_client() -> Option<&'static reqwest::blocking::Client> {
    TELEMETRY_HTTP_CLIENT
        .get_or_init(|| {
            let built = reqwest::blocking::Client::builder()
                .user_agent(jcode_provider_core::KCODE_USER_AGENT)
                .build();
            match built {
                Ok(client) => Some(client),
                Err(err) => {
                    logging::warn(&format!("telemetry HTTP client build failed: {err}"));
                    None
                }
            }
        })
        .as_ref()
}

#[cfg(not(test))]
pub(crate) fn post_payload(payload: serde_json::Value, timeout: Duration) -> bool {
    if TELEMETRY_PERMANENTLY_REJECTED.load(Ordering::Relaxed) {
        return false;
    }
    let breaker_exempt = payload_is_breaker_exempt(&payload);
    let Some(client) = http_client() else {
        logging::warn("telemetry HTTP client failed to build; dropping payload");
        return false;
    };
    match client
        .post(TELEMETRY_ENDPOINT)
        .timeout(timeout)
        .json(&payload)
        .send()
    {
        Ok(response) if response.status().is_success() => true,
        Ok(response) => {
            let status = response.status();
            if telemetry_status_is_permanent(status.as_u16()) && !breaker_exempt {
                TELEMETRY_PERMANENTLY_REJECTED.store(true, Ordering::Relaxed);
                logging::warn(&format!(
                    "telemetry endpoint permanently rejected payload with HTTP {status}; suppressing telemetry delivery for this process"
                ));
            } else {
                logging::warn(&format!(
                    "telemetry endpoint temporarily rejected payload with HTTP {status}"
                ));
            }
            false
        }
        Err(err) => {
            logging::warn(&format!("telemetry payload send failed: {err}"));
            false
        }
    }
}

#[cfg(not(test))]
pub(crate) fn post_payload_with_retry(payload: serde_json::Value, timeout: Duration) -> bool {
    const RETRY_DELAYS: [Duration; 2] = [Duration::from_millis(200), Duration::from_millis(800)];
    if post_payload(payload.clone(), timeout) {
        return true;
    }
    for delay in RETRY_DELAYS {
        if TELEMETRY_PERMANENTLY_REJECTED.load(Ordering::Relaxed) {
            return false;
        }
        std::thread::sleep(delay);
        if post_payload(payload.clone(), timeout) {
            return true;
        }
    }
    false
}

#[cfg(not(test))]
pub(crate) fn post_transcript_payload(payload: serde_json::Value, timeout: Duration) -> bool {
    let Some(client) = http_client() else {
        logging::warn("telemetry HTTP client failed to build; dropping transcript");
        return false;
    };
    match client
        .post(TRANSCRIPT_ENDPOINT)
        .timeout(timeout)
        .json(&payload)
        .send()
    {
        Ok(response) if response.status().is_success() => true,
        Ok(response) => {
            logging::warn(&format!(
                "transcript endpoint rejected upload with HTTP {}",
                response.status()
            ));
            false
        }
        Err(err) => {
            logging::warn(&format!("transcript upload failed: {err}"));
            false
        }
    }
}

pub(crate) fn telemetry_status_is_permanent(status: u16) -> bool {
    (400..500).contains(&status) && !matches!(status, 408 | 425 | 429)
}

pub(crate) fn spawn_background_worker<F>(
    capacity: usize,
    mut deliver: F,
) -> std::io::Result<SyncSender<Value>>
where
    F: FnMut(Value) + Send + 'static,
{
    let (sender, receiver) = sync_channel(capacity);
    std::thread::Builder::new()
        .name("kcode-telemetry".to_string())
        .spawn(move || {
            while let Ok(payload) = receiver.recv() {
                deliver(payload);
            }
        })?;
    Ok(sender)
}

/// Lazily start a telemetry background sender, returning `None` when the
/// worker thread cannot be spawned. The fallback lets callers drop payloads
/// rather than crash the host on thread-creation failure.
#[cfg(not(test))]
pub(crate) fn init_background_sender<F>(capacity: usize, deliver: F) -> Option<SyncSender<Value>>
where
    F: FnMut(Value) + Send + 'static,
{
    match spawn_background_worker(capacity, deliver) {
        Ok(sender) => Some(sender),
        Err(err) => {
            logging::warn(&format!(
                "telemetry background worker failed to start: {err}"
            ));
            None
        }
    }
}

/// Shared shape for the telemetry and transcript background senders: lazily
/// initialize one OnceLock-held worker and borrow it.
#[cfg(not(test))]
pub(crate) fn sender<F>(
    slot: &OnceLock<Option<SyncSender<Value>>>,
    capacity: usize,
    deliver: F,
) -> Option<&SyncSender<Value>>
where
    F: FnMut(Value) + Send + 'static,
{
    slot.get_or_init(|| init_background_sender(capacity, deliver))
        .as_ref()
}

#[cfg(not(test))]
pub(crate) fn background_sender() -> Option<&'static SyncSender<Value>> {
    // A worker thread has no caller to report to: the delivery result is
    // deliberately not inspected here, and both callees already log their
    // own failure. Naming the call is enough; binding it to `_` would be
    // counted by the swallowed-error ratchet.
    sender(
        &TELEMETRY_BACKGROUND_SENDER,
        BACKGROUND_QUEUE_CAPACITY,
        |payload| {
            post_payload_with_retry(payload, ASYNC_SEND_TIMEOUT);
        },
    )
}

#[cfg(not(test))]
pub(crate) fn transcript_background_sender() -> Option<&'static SyncSender<Value>> {
    sender(&TRANSCRIPT_BACKGROUND_SENDER, 64, |payload| {
        post_transcript_payload(payload, ASYNC_SEND_TIMEOUT);
    })
}

/// Delivery state shared with the parent module's `send_payload`. Re-exported
/// so the queue/overflow flags live with the rest of the delivery surface
/// instead of inflating `lib.rs`.
#[cfg(not(test))]
use delivery_state::{
    ASYNC_SEND_TIMEOUT, BACKGROUND_QUEUE_CAPACITY, TELEMETRY_BACKGROUND_SENDER,
    TELEMETRY_HTTP_CLIENT, TELEMETRY_PERMANENTLY_REJECTED, TELEMETRY_QUEUE_OVERFLOW_WARNED,
    TRANSCRIPT_BACKGROUND_SENDER,
};

pub(crate) fn payload_is_breaker_exempt(payload: &serde_json::Value) -> bool {
    matches!(
        payload.get("event").and_then(|value| value.as_str()),
        Some("usage_report")
    )
}

#[cfg(not(test))]
pub(crate) fn send_payload(mut payload: serde_json::Value, mode: DeliveryMode) -> bool {
    super::concurrency::mark_legacy_concurrency_unavailable(&mut payload);
    match mode {
        DeliveryMode::Background => {
            if TELEMETRY_PERMANENTLY_REJECTED.load(Ordering::Relaxed) {
                return false;
            }
            logging::debug("queueing telemetry payload for background delivery");
            let Some(sender) = background_sender() else {
                logging::warn("telemetry background worker unavailable; dropping payload");
                return false;
            };
            match sender.try_send(payload) {
                Ok(()) => {
                    TELEMETRY_QUEUE_OVERFLOW_WARNED.store(false, Ordering::Relaxed);
                    true
                }
                Err(TrySendError::Full(_)) => {
                    if !TELEMETRY_QUEUE_OVERFLOW_WARNED.swap(true, Ordering::Relaxed) {
                        logging::warn(&format!(
                            "telemetry background queue is full (capacity={BACKGROUND_QUEUE_CAPACITY}); dropping events until delivery catches up"
                        ));
                    }
                    false
                }
                Err(TrySendError::Disconnected(_)) => {
                    logging::warn("telemetry background worker stopped; dropping payload");
                    false
                }
            }
        }
        DeliveryMode::Blocking(timeout) => {
            logging::debug(&format!(
                "sending telemetry payload with blocking timeout={}ms",
                timeout.as_millis()
            ));
            if tokio::runtime::Handle::try_current().is_ok() {
                let (tx, rx) = std::sync::mpsc::sync_channel(1);
                std::thread::spawn(move || {
                    let _ = tx.send(post_payload(payload, timeout));
                });
                rx.recv_timeout(timeout).unwrap_or(false)
            } else {
                post_payload(payload, timeout)
            }
        }
    }
}
