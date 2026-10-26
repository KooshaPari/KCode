//! Socket-level integration coverage for the daemon identity seam.
//!
//! The unit tests in `daemon_identity.rs` decide with in-memory identities.
//! These tests cover the layer underneath with what a running daemon really
//! produces: a real Unix socket carrying the real ping/pong liveness handshake,
//! and a real identity sidecar written and read by the production helpers.
//! The peer speaks the production wire protocol and nothing else, so a test can
//! tell a liveness probe apart from session traffic by counting pings.

#![cfg(unix)]

use super::{AttachPlan, attach_plan, canonical_provider_key, running_daemon_identity};
use crate::cli::commands::tests::SavedEnv;
use crate::cli::provider_init::ProviderChoice;
use crate::server::{
    SOCKET_ENV_VAR, SocketIdentity, has_live_listener, identity_file_for_socket, is_server_ready,
    read_socket_identity, write_socket_identity,
};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// What the peer observed, so a test can tell a probe from session traffic.
#[derive(Debug, Default, Clone)]
struct PeerState {
    connections: usize,
    pings: usize,
    last_ping_id: Option<u64>,
}

/// One test's world: the socket directory, the peer, and the sidecar.
struct Fixture {
    dir: PathBuf,
    socket: PathBuf,
    state: Arc<Mutex<PeerState>>,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Fixture {
    fn start(tag: &str) -> Self {
        // The socket path must stay under the 104-byte sun_path limit.
        let dir = std::env::temp_dir().join(format!("jid-{}-{tag}", std::process::id()));
        // A run that died mid-test leaves the directory behind, and binding
        // over a live socket file would fail.
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create the peer socket directory");
        let socket = dir.join("s.sock");
        assert!(socket.as_os_str().len() < 100, "socket path too long");
        let listener = UnixListener::bind(&socket).expect("bind the test peer socket");
        let state = Arc::new(Mutex::new(PeerState::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_state = Arc::clone(&state);
        let thread_stop = Arc::clone(&stop);
        let handle = std::thread::spawn(move || serve_peer(listener, thread_state, thread_stop));
        Self {
            dir,
            socket,
            state,
            stop,
            handle: Some(handle),
        }
    }

    fn socket(&self) -> &Path {
        &self.socket
    }

    /// Publish an identity beside the socket, the way a daemon does.
    fn publish(&self, provider: &str, model: Option<&str>) {
        let identity = SocketIdentity {
            provider: provider.to_string(),
            model: model.map(str::to_string),
        };
        write_socket_identity(&self.socket, &identity).expect("write the sidecar");
    }

    /// The identity a client reads off the shared socket.
    fn daemon(&self) -> Option<SocketIdentity> {
        running_daemon_identity(&self.socket)
    }

    fn observed(&self) -> PeerState {
        self.state.lock().expect("the peer state lock").clone()
    }

    /// Stop the peer and wait for its thread to finish.
    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // A throwaway connection wakes the accept loop up.
        let _ = UnixStream::connect(&self.socket);
        if let Some(handle) = self.handle.take() {
            handle.join().expect("the peer thread must not panic");
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.shutdown();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn serve_peer(listener: UnixListener, state: Arc<Mutex<PeerState>>, stop: Arc<AtomicBool>) {
    loop {
        let (stream, _) = match listener.accept() {
            Ok(pair) => pair,
            Err(_) => break,
        };
        if stop.load(Ordering::SeqCst) {
            // The throwaway connection that unblocks the accept loop.
            break;
        }
        let mut state = state.lock().expect("the peer state lock");
        state.connections += 1;
        let Ok(write_half) = stream.try_clone() else {
            continue;
        };
        let mut writer = write_half;
        let reader = BufReader::new(stream);
        for line in reader.lines() {
            let Ok(line) = line else {
                break;
            };
            if !line.contains("\"ping\"") {
                continue;
            }
            let Some(id) = parse_request_id(&line) else {
                continue;
            };
            let mut state = state.lock().expect("the peer state lock");
            state.pings += 1;
            state.last_ping_id = Some(id);
            let reply = format!("{{\"type\":\"pong\",\"id\":{id}}}\n");
            if writer.write_all(reply.as_bytes()).is_err() {
                break;
            }
            let _ = writer.flush();
        }
    }
}

/// Pull the request id out of a wire line without pulling in a JSON parser.
fn parse_request_id(line: &str) -> Option<u64> {
    let marker = "\"id\":";
    let start = line.find(marker)? + marker.len();
    let digits: String = line[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// Ask the peer for a pong and report whether it answered with the same id.
fn ping_peer(socket: &Path, id: u64) -> bool {
    let Ok(mut stream) = UnixStream::connect(socket) else {
        return false;
    };
    let request = format!("{{\"type\":\"ping\",\"id\":{id}}}\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let _ = stream.flush();
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return false;
    }
    line.contains("\"pong\"") && parse_request_id(&line) == Some(id)
}

/// A concrete provider request whose canonical key comes from the production
/// mapping, so the test cannot drift from the key a daemon publishes.
fn concrete_choice() -> ProviderChoice {
    ProviderChoice::Cursor
}

fn canonical_key(choice: &ProviderChoice) -> String {
    canonical_provider_key(choice)
        .expect("a concrete provider choice must have a canonical runtime key")
        .to_string()
}

/// The bug this seam exists for: a client that asked for one provider must not
/// be captured by a daemon serving another, but it must attach when the
/// daemon really does serve what was asked for.
#[tokio::test]
async fn attaches_when_the_sidecar_matches_the_requested_provider() {
    let _env_lock = crate::storage::lock_test_env();
    let fixture = Fixture::start("match");
    let choice = concrete_choice();
    let key = canonical_key(&choice);
    fixture.publish(&key, Some("gpt-5"));

    let daemon = fixture.daemon().expect("the sidecar must be readable back");
    assert_eq!(
        daemon.provider, key,
        "the sidecar must report the daemon provider"
    );
    assert_eq!(daemon.model.as_deref(), Some("gpt-5"), "the served model");
    let read = read_socket_identity(fixture.socket()).expect("read the sidecar back");
    assert_eq!(read, daemon, "the sidecar must round-trip exactly");
    assert_eq!(
        attach_plan(Some(&choice), Some(&daemon)),
        AttachPlan::Attach,
        "a daemon publishing the requested provider must be attached to"
    );
    assert!(
        is_server_ready(fixture.socket()).await,
        "the production liveness handshake must reach the peer"
    );
    assert!(
        fixture.observed().pings >= 1,
        "the peer must have answered the liveness ping"
    );

    // `auto` claims no identity, so it attaches even to a daemon that serves
    // another provider.
    fixture.publish("another-provider", None);
    let daemon = fixture.daemon().expect("the sidecar must be readable back");
    assert_eq!(
        attach_plan(Some(&ProviderChoice::Auto), Some(&daemon)),
        AttachPlan::Attach,
        "an auto request claims no identity and must attach"
    );
}

/// A daemon serving another provider must never capture a client that asked
/// for one provider explicitly.
#[tokio::test]
async fn isolates_when_the_sidecar_names_another_provider() {
    let _env_lock = crate::storage::lock_test_env();
    let fixture = Fixture::start("mismatch");
    let choice = concrete_choice();
    fixture.publish("definitely-not-the-requested-provider", None);

    let daemon = fixture.daemon().expect("the sidecar must be readable back");
    assert_eq!(daemon.model, None, "an absent model round-trips");
    assert_eq!(
        attach_plan(Some(&choice), Some(&daemon)),
        AttachPlan::Isolate,
        "a daemon serving another provider must not capture this client"
    );
    assert_eq!(
        fixture.observed().pings,
        0,
        "the decision path must not send session traffic to the shared daemon"
    );
}

/// An older daemon publishes no sidecar and a corrupt one proves nothing
/// either, so neither can be read as agreement.
#[tokio::test]
async fn isolates_when_the_sidecar_is_missing_or_malformed() {
    let _env_lock = crate::storage::lock_test_env();
    let choice = concrete_choice();

    let missing = Fixture::start("missing");
    let sidecar = identity_file_for_socket(missing.socket());
    assert!(!sidecar.exists(), "no sidecar");
    assert!(missing.daemon().is_none(), "unknown identity");
    assert_eq!(
        attach_plan(Some(&choice), None),
        AttachPlan::Isolate,
        "an unknown daemon identity must not be treated as agreement"
    );

    let malformed = Fixture::start("malformed");
    let sidecar = identity_file_for_socket(malformed.socket());
    std::fs::write(&sidecar, b"{not json").expect("write a malformed sidecar");
    assert!(malformed.daemon().is_none(), "unreadable identity");
    assert_eq!(
        attach_plan(Some(&choice), None),
        AttachPlan::Isolate,
        "a malformed sidecar must not be treated as agreement"
    );
}

/// The production liveness path must find the peer through the socket env var.
#[tokio::test]
async fn the_production_liveness_path_finds_the_peer() {
    let _env_lock = crate::storage::lock_test_env();
    let fixture = Fixture::start("liveness");
    // The guard restores the variable even when an assertion panics.
    let _saved = SavedEnv::capture(&[SOCKET_ENV_VAR]);
    crate::env::set_var(SOCKET_ENV_VAR, &fixture.socket);

    assert!(
        is_server_ready(fixture.socket()).await,
        "the production ping/pong handshake must reach the peer"
    );
    assert!(
        has_live_listener(fixture.socket()).await,
        "the production listener probe must find the peer"
    );
    assert!(
        crate::cli::dispatch::server_is_running().await,
        "the production liveness path must find the peer through the socket env var"
    );
    assert!(
        fixture.observed().pings >= 1,
        "the peer must have answered the liveness ping"
    );
}

/// Isolating a client must never take the shared daemon down with it.
#[tokio::test]
async fn the_shared_daemon_survives_every_decision() {
    let _env_lock = crate::storage::lock_test_env();
    let fixture = Fixture::start("survival");
    let choice = concrete_choice();

    fixture.publish(&canonical_key(&choice), None);
    let daemon = fixture.daemon().expect("the sidecar must be readable back");
    assert_eq!(
        attach_plan(Some(&choice), Some(&daemon)),
        AttachPlan::Attach
    );

    fixture.publish("another-provider", None);
    let daemon = fixture.daemon().expect("the sidecar must be readable back");
    assert_eq!(
        attach_plan(Some(&choice), Some(&daemon)),
        AttachPlan::Isolate
    );

    assert!(
        fixture.handle.is_some(),
        "the shared daemon must still be running"
    );
    assert!(
        is_server_ready(fixture.socket()).await,
        "the shared daemon must still answer the production liveness handshake"
    );
    assert!(
        ping_peer(fixture.socket(), 42),
        "the shared daemon must still answer a ping"
    );
    let observed = fixture.observed();
    assert!(
        observed.connections >= 2,
        "the peer must have accepted the survival probe, saw {}",
        observed.connections
    );
    assert_eq!(observed.last_ping_id, Some(42));
}
