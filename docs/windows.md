# Windows

Synapse builds for Windows x64 with the MSVC Rust toolchain. The desktop needs
Direct3D 11. The CLI is packaged separately and works without a graphics session.

## Build an installer

Install Rust stable for `x86_64-pc-windows-msvc`, Visual Studio Build Tools with
Desktop development with C++ and a Windows SDK, and Inno Setup 6. The SDK must
include `fxc.exe`; set `GPUI_FXC_PATH` if GPUI cannot find it.

From PowerShell at the repository root:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\packaging\windows\build.ps1
```

The script builds both binaries with a static C runtime and writes
`dist\<version>\synapse.<version>.windows.x64.exe` and `sha256sums.txt`.
Use `-NoBuild` to package binaries already built with the same target and flags.
`CARGO_TARGET_DIR` is supported. The release workflow builds and uploads this
installer before publishing a release. Windows installers currently ship unsigned.

The installer runs per user without elevation, creates a Start menu shortcut,
and offers a desktop shortcut and user PATH entry. It installs the app under
`%LOCALAPPDATA%\Programs\Synapse`, with the CLI in `bin\synapse.exe`.
Open a new terminal after installation to use `synapse` by name.
Uninstall removes the program and the PATH entry it added, preserving user data.
The installer includes the pi extension. Windows connections use that bundled
package; `SYNAPSE_PI_PACKAGE` still overrides its source for development.

Synapse stores its database, encrypted vault, and shared guidance under
`%LOCALAPPDATA%\synapse`. The encrypted vault is the Windows backend; Apple's
Keychain backend is available only on macOS. Windows editor defaults to Notepad.
Bash, zsh, and fish activation hooks are for those shells; Windows users can use
`synapse run -- <program> <arguments>` for credential injection.

## Verify

```powershell
cargo test --locked
cargo test --locked --manifest-path crates/synapsecore/Cargo.toml
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\packaging\windows\smoke.ps1 -Cli "$env:LOCALAPPDATA\Programs\Synapse\bin\synapse.exe"
```

The smoke check uses synthetic data in an isolated temporary directory. It
verifies memory, vault injection, backup and restore, and repeatable CLI installation.
The CI workflow runs the desktop and core suites on Windows.
