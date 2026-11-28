# Opt-in provider proxy qualification

## Scope and identity

Repository: KooshaPari/KCode; its source checkout is named `jcode`.
Verified fork origin: `https://github.com/KooshaPari/KCode.git`.
Baseline/default branch: `master` at
`028ae83b769a932c013f527b305a5dbf0480ffe0`.
Worktree: isolated branch worktree for this qualification change.
Branch: `codex/qualification-no-proxy-20261018`.
The canonical checkout, unrelated untracked documents, installed profiles,
launchers and shared services were preserved. No upstream publication occurred.

## Root cause and supported API

The supplied sanitized receipt identifies the startup path:
`Server::new_with_name -> AmbientRunnerHandle::new -> NotificationDispatcher::new
-> jcode_provider_core::shared_http_client -> reqwest ClientBuilder::build
-> system proxy -> SystemConfiguration NULL-object panic` when Mach is denied.
The receipt's installed-binary SHA is
`d649bc210a4dbcc29d6e501208d683d45e4a6191ba8a0bc58a2df32e8f89cb5a`;
this does not identify that binary's source revision or qualify a repaired install.

The provider crate enables system proxies and pins reqwest 0.12.28 with
hyper-util 0.1.19. Inspection of matching registry sources confirms that reqwest's
`auto_sys_proxy` defaults true; `build()` adds the system matcher only when this
flag remains true. `no_proxy()` clears proxies and sets it false. Hyper-util's
macOS system matcher calls `SCDynamicStoreBuilder::new("").build()` even after
reading environment proxy settings.

Primary reference: [reqwest 0.12.28 ClientBuilder::no_proxy](https://docs.rs/reqwest/0.12.28/reqwest/struct.ClientBuilder.html#method.no_proxy).
Matching upstream source locations are reqwest `src/async_impl/client.rs` and
hyper-util `src/client/proxy/matcher.rs`; their local registry sources were read.
The official API page was accessible; online source-page fetches were unavailable.

## Implementation

Only exact `KCODE_QUALIFICATION_NO_PROXY=1` invokes the documented `no_proxy()`
API for generic shared/fresh provider clients and fallback construction.
Unset/other values return ordinary reqwest builders, retaining proxy defaults.
The shared cache, User-Agent, timeouts, HTTP/2 keepalive and pooling are unchanged.
No dependency feature defaults or OS sandbox permissions changed.

Set the opt-in before isolated child startup. It is not an offline/network
security boundary: the qualification sandbox must continue denying outbound
network and Mach access. Other independently constructed provider clients and
full daemon/session behavior are outside this narrowly scoped repair.

## Verification and limits

Tests isolate process environment and process-wide client caches. The exact
baseline produced a clean RED: 1 passed, 2 intended failures; opt-in still used
the loopback proxy and still reproduced the SystemConfiguration NULL panic.

After the production patch, the Mach-denied test passed. A separate fixture read
returned WouldBlock and cleanup double-panicked. The failed log was preserved.
The authorized fixture repair explicitly switches accepted sockets to blocking
before their bounded read and avoids a second panic while already unwinding.

Final commands, with the isolated cache and macOS SDK/deployment environment:

```sh
cargo test --locked --jobs 1 -p jcode-provider-core --test qualification_proxy
cargo check -p jcode-provider-core --jobs 1
```

Focused tests: **3 passed, 0 failed**, 1.45s runtime; cached compile 3.03s.
Check: **passed**, exit 0, 37.00s. Formatting and `git diff --check` passed.

The real clients preserve unset/invalid-value loopback proxy routing, bypass it
only for exact opt-in, and build under identical Mach/network denial only with
opt-in. Default mode retains the known NULL-panic negative control. The shared
client is called twice and the fresh-client path is exercised. No provider
credentials or model requests are used. No whole suite, full ACP/daemon startup,
installed artifact, hosted CI, or release gate is claimed.

## Preserved artifacts

Preserved local logs, retained outside the source worktree:

- `kcode-proxy-resume-red.log`: clean baseline RED.
- `kcode-proxy-green.log`: preserved fixture failure; SHA-256
  `64321fb06b485db9223563ee88c2cbdb4c6abdcfcf831609585517c8c5c55e2a`.
- `kcode-proxy-fixture-green.log`: final focused GREEN.
- `kcode-proxy-check.log`: successful crate check.

Local compute used one job, no GPU or remote compute. The validation build cache
was moved intact from the isolated worktree to a local state directory after
validation so it remains outside the reviewable source tree. No cache data was
deleted.
