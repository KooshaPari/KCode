# Remote kcode (cross-machine)

Drive a kcode session running on **another machine** from this one, over the
WebSocket gateway (`crates/kcode-base/src/gateway.rs`).

Verified working: Linux client -> Windows 11 host over Tailscale, including
remote `bash` tool execution, ~10ms WebSocket upgrade.

## Setup on the host (the machine that runs the session)

From inside kcode, `/remote` does all of this:

```
/remote on        # enable the gateway
                  # then restart the server: kcode server reload
/remote pair      # pairing code + QR
/remote           # status, dial address, paired devices
/remote revoke <device>
```

The equivalent by hand:

1. Enable the gateway in `~/.kcode/config.toml` (Windows: `%USERPROFILE%\.kcode\config.toml`):

   ```toml
   [gateway]
   enabled = true
   port = 7643
   bind_addr = "0.0.0.0"
   ```

   `KCODE_GATEWAY_ENABLED=1` works as an env override.

2. Restart the server so it picks up the config:

   ```sh
   kcode server reload      # or: kcode server stop --force && kcode server start
   ```

   Confirm it bound: `curl -s http://127.0.0.1:7643/health`

3. Generate a pairing code (valid 5 minutes):

   ```sh
   kcode pair
   ```

## Setup on the client

```sh
python3 scripts/remote/remote_check.py --host <host-ip> --code 123456 \
    --working-dir '/path/on/remote'
```

The token is cached in `~/.kcode/remote-tokens/<host>_<port>.json` (mode 600),
so later runs omit `--code`.

## Gotchas found in practice

- **Windows Firewall silently wins.** Auto-created inbound *block* rules for
  `kcode.exe` (created when someone dismisses the Windows network prompt)
  override any allow rule you add. `/health` will answer on the host itself and
  time out from outside. Find and remove them:

  ```powershell
  Get-NetFirewallRule -Direction Inbound -Action Block -Enabled True |
    Where-Object { ($_ | Get-NetFirewallApplicationFilter).Program -like '*kcode*' } |
    Remove-NetFirewallRule
  ```

  Then allow the port, scoped to the Tailscale CGNAT range rather than the world:

  ```powershell
  netsh advfirewall firewall add rule name="kcode-gateway-tailscale" `
    dir=in action=allow protocol=TCP localport=7643 remoteip=100.64.0.0/10
  ```

- **`bind_addr = "0.0.0.0"` is not authentication.** Anything that can reach the
  port can attempt to pair. Keep it on a private network (Tailscale) and scope
  the firewall rule; do not expose 7643 to the public internet.

- **History is not pushed on `subscribe`.** Subscribe only acks; clients must
  send `get_history` explicitly.

- **The server pings.** A client that never sends a pong frame will appear to
  hang with zero events. `gateway_client.py` answers pings for you.

- **`kcode server reload` can fail** with "Client must Subscribe with a
  working_dir" on older builds. Fall back to `stop --force` + `start`.
