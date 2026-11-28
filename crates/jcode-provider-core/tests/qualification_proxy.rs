//! Process-isolated proxy qualification; no provider credentials or model calls.
use jcode_provider_core::{fresh_transport_client, shared_http_client};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Output},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const OPT_IN: &str = "KCODE_QUALIFICATION_NO_PROXY";
const PROBE: &str = "KCODE_PROXY_TEST_PROBE";

fn child(mode: Option<&str>) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .env_clear()
        .args(["--exact", "client_probe", "--nocapture", "--test-threads=1"])
        .env(PROBE, "build")
        .env("PATH", "/usr/bin:/bin");
    if let Some(value) = mode {
        command.env(OPT_IN, value);
    }
    command
}

fn success(output: Output) {
    assert!(
        output.status.success(),
        "child failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("PROVIDER_CLIENTS_BUILT"));
}

#[tokio::test]
async fn client_probe() {
    let Ok(probe) = std::env::var(PROBE) else {
        return;
    };
    // Call twice to exercise the process-wide cache, then its fresh retry path.
    let clients = [
        shared_http_client(),
        shared_http_client(),
        fresh_transport_client(),
    ];
    if probe == "network" {
        let url = std::env::var("KCODE_PROXY_TEST_URL").unwrap();
        let expected = std::env::var("KCODE_PROXY_TEST_BODY").unwrap();
        for client in clients {
            let response = client
                .get(&url)
                .timeout(Duration::from_secs(3))
                .send()
                .await
                .unwrap();
            assert_eq!(response.text().await.unwrap(), expected);
        }
    }
    println!("PROVIDER_CLIENTS_BUILT");
}

struct Server {
    address: String,
    requests: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Server {
    fn new(body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let requests = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (count, stopping) = (requests.clone(), stop.clone());
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(30);
            while !stopping.load(Ordering::Relaxed) && Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // Accepted sockets can inherit O_NONBLOCK on macOS.
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut request = [0; 16384];
                        let bytes = stream.read(&mut request).unwrap();
                        assert!(bytes > 0);
                        count.fetch_add(1, Ordering::Relaxed);
                        write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .unwrap();
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("loopback fixture: {e}"),
                }
            }
        });
        Self {
            address,
            requests,
            stop,
            worker: Some(worker),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Err(failure) = self.worker.take().unwrap().join() {
            eprintln!("loopback fixture worker panicked");
            // Keep the original test failure visible instead of aborting in Drop.
            if !thread::panicking() {
                std::panic::resume_unwind(failure);
            }
        }
    }
}

#[test]
fn opt_in_bypasses_proxy_while_unset_and_other_values_preserve_it() {
    let proxy = Server::new("proxy");
    let origin = Server::new("direct");
    for (mode, body) in [
        (None, "proxy"),
        (Some("true"), "proxy"),
        (Some("1"), "direct"),
    ] {
        let output = child(mode)
            .env(PROBE, "network")
            .env("HTTP_PROXY", format!("http://{}", proxy.address))
            .env("NO_PROXY", "")
            .env(
                "KCODE_PROXY_TEST_URL",
                format!("http://{}/qualification", origin.address),
            )
            .env("KCODE_PROXY_TEST_BODY", body)
            .output()
            .unwrap();
        success(output);
    }
    assert_eq!(proxy.requests.load(Ordering::Relaxed), 6);
    assert_eq!(origin.requests.load(Ordering::Relaxed), 3);
}

#[cfg(target_os = "macos")]
#[test]
fn mach_denied_opt_in_builds_real_clients_and_normal_mode_keeps_system_probe() {
    // Both modes get the identical restrictions. No Mach or network grant is added.
    let profile = "(version 1) (allow default) (deny mach-lookup) (deny network*)";
    let restricted = |mode: Option<&str>| {
        let probe = child(mode);
        let mut command = Command::new("/usr/bin/sandbox-exec");
        command
            .env_clear()
            .args(["-p", profile])
            .arg(probe.get_program())
            .args(probe.get_args());
        for (key, value) in probe.get_envs() {
            if let Some(value) = value {
                command.env(key, value);
            }
        }
        command.output().unwrap()
    };
    let normal = restricted(None);
    assert!(
        !normal.status.success(),
        "negative control no longer reproduces denied system-proxy discovery"
    );
    let stderr = String::from_utf8_lossy(&normal.stderr);
    assert!(
        stderr.contains("Attempted to create a NULL object"),
        "unexpected failure: {stderr}"
    );
    success(restricted(Some("1")));
}
