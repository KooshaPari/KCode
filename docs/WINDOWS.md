# KCode on Windows

KCode is a separate application from stock jcode. Use `kcode.exe`, KCode
storage, and this checkout's installer. The upstream `jcode.sh` installer,
Homebrew tap, and `jcode-install` package install stock jcode.

## Current support and installation

At the 2026-10-21 hosted-source audit, the root package is
`0.93.0-k0.1.0`. A complete Windows release, native ConPTY behavior, and
installation on the operator's device remain unverified. Fork downloads and
self-update are deliberately disabled until a maintained channel passes the
[KCode isolation and delivery gates](KCODE_ISOLATION.md#forward-delivery-work-breakdown).

Build an explicitly trusted checkout with Git, Rust, and Visual Studio 2022
Build Tools with the **Desktop development with C++** workload:

```powershell
cargo build --release --locked --bin kcode
if ($LASTEXITCODE -ne 0) { throw "KCode build failed" }
$artifactVersion = ((& .\target\release\kcode.exe --version) -split '\s+')[1]
.\scripts\install.ps1 -ArtifactExePath .\target\release\kcode.exe -Version $artifactVersion
```

The local-artifact installer uses these independent destinations:

| Resource | Destination |
| --- | --- |
| Launcher | `%LOCALAPPDATA%\kcode\bin\kcode.exe` |
| Stable binary | `%LOCALAPPDATA%\kcode\builds\stable\kcode.exe` |
| Immutable binary | `%LOCALAPPDATA%\kcode\builds\versions\<version>\kcode.exe` |
| User configuration and sessions | `%USERPROFILE%\.kcode` |

The installer adds only its KCode bin directory to the user and current-process
PATH. Open a new PowerShell session to verify persistent command resolution.
Alacritty and global launch-hotkey setup are opt-in; standard Windows Terminal
does not require them.

```powershell
Get-Command kcode -All
kcode --version
Get-FileHash (Get-Command kcode).Source -Algorithm SHA256
Get-AuthenticodeSignature (Get-Command kcode).Source |
    Format-List Status,StatusMessage,SignerCertificate
```

Record the resolved client executable and the separately running server's
executable, version, and IPC address. Changing PATH alone does not prove the
long-lived server has changed. Preserve existing jcode data and processes.

## Required remote topology

The Mac is the remote client viewing or controlling the Windows HERDR host:

```text
Mac client --> remote transport --> Windows HERDR host --> Windows KCode server
```

Standard Windows Terminal and local Windows HERDR behavior are independent
controls. Remote acceptance must verify Windows server transport and HERDR
integration from the Mac, including input, paste, resize, reconnect and session
continuity. Record client and host server identities separately. Native Unix
SSH-client support is not the primary blocker for this deployment. A
Windows-to-Mac test does not satisfy this acceptance route.

## Inherited upstream security and release reference

The sections below describe upstream capabilities and acceptance requirements.
They do not establish that KCode has published signed Windows assets, enabled
updates, or passed Windows CI. Apply the fork-specific gates linked above before
claiming release readiness.

## Microsoft Defender and SmartScreen

Two different Windows warnings are commonly confused:

- **Microsoft Defender SmartScreen** shows messages such as “Windows protected your PC” when a downloaded application is unsigned or has not accumulated enough publisher reputation. Authenticode signing with a trusted, timestamped certificate is the primary fix. Reputation still builds over time for a new publisher identity.
- **Microsoft Defender Antivirus** reports a named threat or suspicious behavior. Signing helps establish provenance, but a heuristic false positive must also be submitted to Microsoft with the exact signed file and SHA-256 hash.

Do not tell users to disable Defender, add exclusions, or bypass a named malware detection. First verify the release URL, checksum, and Authenticode signature. If a correctly signed official build is still detected, submit it through the [Microsoft Security Intelligence file submission portal](https://www.microsoft.com/wdsi/filesubmission) as a software developer false positive.

### Heuristic-trigger reduction already in place

The Windows setup is deliberately designed to avoid unnecessary suspicious behavior:

- Release downloads are verified against `SHA256SUMS`.
- Optional terminal and global-hotkey setup requires explicit consent.
- The old hidden VBScript startup trampoline has been removed.
- The hotkey listener uses a direct PowerShell shortcut with `RemoteSigned`, not `ExecutionPolicy Bypass`.
- Release binaries are built on GitHub-hosted Windows runners and tested before publication.

## Enable Authenticode signing

The release workflow supports [Azure Artifact Signing](https://azure.microsoft.com/products/artifact-signing) with GitHub OIDC. This keeps the certificate private key in Microsoft's managed signing service instead of exporting it into a GitHub secret.

This is a one-time owner setup and may require Azure billing and organization or identity verification:

1. Create an Artifact Signing account and a public-trust certificate profile.
2. Create a Microsoft Entra application or managed identity with a federated credential for `1jehuang/jcode` GitHub Actions.
3. Grant it the **Artifact Signing Certificate Profile Signer** role on the certificate profile.
4. Add these GitHub Actions secrets:
   - `AZURE_CLIENT_ID`
   - `AZURE_TENANT_ID`
   - `AZURE_SUBSCRIPTION_ID`
5. Add these GitHub Actions variables:
   - `WINDOWS_SIGNING_ENDPOINT`, for example `https://eus.codesigning.azure.net/`
   - `WINDOWS_SIGNING_ACCOUNT`
   - `WINDOWS_SIGNING_CERTIFICATE_PROFILE`
6. Push a test tag and confirm the `Sign and publish Windows assets` job signs both executables and that `Get-AuthenticodeSignature` returns `Valid`.
7. Leave `WINDOWS_SIGNING_REQUIRED` unset or set it to `true`. Signing is required by default for Windows assets, so missing configuration or a signing outage omits Windows from that release while every other successful platform can still publish. Setting it to `false` is an explicit emergency override and is not suitable for an official Windows release.

The workflow applies SHA-256 Authenticode signatures and RFC 3161 timestamps before packaging, checksum generation, and release upload. Both x64 and ARM64 executables are signed on a supported x64 Windows signing runner.

Do not describe the Defender and SmartScreen rollout as complete until signing enforcement is active and a public release has a valid signature.

## Release acceptance checklist

For every release that changes Windows behavior:

- [ ] Windows x64 CI build and targeted tests pass.
- [ ] Windows lifecycle end-to-end tests pass.
- [ ] x64 and ARM64 installer verification passes.
- [ ] Both `.exe` files have a valid, timestamped Authenticode signature.
- [ ] `SHA256SUMS` contains both Windows executables and archives.
- [ ] A clean Windows 11 machine installs, launches, updates, and uninstalls successfully.
- [ ] Defender Antivirus reports no named detection on the signed release.
- [ ] SmartScreen identifies the expected publisher. Any low-reputation warning is tracked separately from malware detection.
- [ ] The website Windows button points to a published asset and contains no preview or work-in-progress wording.
- [ ] Release notes mention material Windows fixes or limitations.

## Continuous integration

Windows is covered by:

- `.github/workflows/ci.yml`: release build, test compilation, targeted platform tests, runtime smoke tests, lifecycle end-to-end tests, installer verification, and PowerShell syntax checks.
- `.github/workflows/windows-smoke.yml`: manually dispatchable x64 and ARM64 smoke validation.
- `.github/workflows/release.yml`: x64 and ARM64 builds, managed signing required for Windows publication, signature verification, packaging, and checksums. Every platform and architecture publishes independently, so a Windows failure cannot block any successful Unix asset.

## Architecture notes

Unix domain sockets are replaced by Windows named pipes under `crates/jcode-base/src/transport/windows.rs`. Platform-specific filesystem, process, update, and replacement behavior is selected at compile time with `#[cfg(windows)]`, so Windows support does not add runtime branching to Unix builds.

Windows launch-hotkey setup is implemented in `crates/jcode-setup-hints/src/windows_setup.rs` and is only installed after explicit user consent.

## Reporting Windows problems

Include the following in a GitHub issue:

- Windows edition, version, and architecture
- Jcode version from `jcode --version`
- Installation method
- Terminal and PowerShell version (`$PSVersionTable.PSVersion`)
- Exact Defender or SmartScreen message
- Defender threat name, if one was shown
- SHA-256 from `Get-FileHash`
- Authenticode status from `Get-AuthenticodeSignature`

Do not upload private configuration, credentials, session transcripts, or `.jcode` authentication files.
