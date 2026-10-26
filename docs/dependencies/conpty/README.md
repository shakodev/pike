# Pinned Windows ConPTY runtime

Pike uses `Microsoft.Windows.Console.ConPTY` **1.25.260930003**, a stable
[Microsoft NuGet package](https://www.nuget.org/packages/Microsoft.Windows.Console.ConPTY/1.25.260930003),
through the existing Alacritty backend. This replaces the inbox API selection,
not the PTY implementation. No Rust crate, Job Object or permanent worker is
added. The package advertises Windows build 17763 and above; verified server
cohorts and remaining client-OS limitations are recorded in issue #55's
[verification](../../verification/issue-55.md).

The bare SDK control in issue #8 reproduces one leaked host handle per console
on Server 2022 build 20348, including failed launches. That measured OS defect
justifies this native dependency. Accessing opaque HPCON internals, guessing
handles, or dropping support for the OS is not required.

## Provenance and shipped files

[tools/conpty-runtime.json](../../../tools/conpty-runtime.json) pins the exact
NuGet URL, archive SHA-256, x64 asset paths, file SHA-256 values and sizes. The
package was downloaded and the bytes verified before adding the manifest.
Its package metadata declares Microsoft as author and MIT as license; the
[license](LICENSE) is retained from the matching upstream release tag
[v1.25.2733.0](https://github.com/microsoft/terminal/releases/tag/v1.25.2733.0).
Package headers explicitly name `ConptyCreatePseudoConsole`,
`ConptyResizePseudoConsole` and `ConptyClosePseudoConsole`; those exact exports
use the SDK-compatible signatures in the adapter. The previous optional
loader's unprefixed symbols would not load this distribution.

| Asset | Bytes | SHA-256 |
| --- | ---: | --- |
| conpty.dll | 119,608 | `feeef341d891643c62d30b6b07800bc70f0bc148f44cb8c3bee84aa557ae805a` |
| OpenConsole.exe | 1,089,376 | `3d66b23d0a71bb8eed2b77edc8b9df9bf54ce6c8fb74c863a30e760997f80586` |

Archive SHA-256: `02b07b349af66d801159bdf9e440d4a1ce78bb951f37fc8609731665afdae7ee`.
The two native assets total 1,208,984 bytes (about 1.15 MiB) on disk. Other
architectures, import libraries, .NET wrappers and package tooling are not
shipped. Import-table inspection of the pinned pair found Windows system/API
set dependencies, including the system Universal CRT, and no separate MSVC
redistributable DLL imports. This does not establish support for untested OS
images or reduced installations.

## Build and deployment policy

Prepare the runtime explicitly before running Windows executables or tests:

```powershell
powershell -NoProfile -File tools/prepare-conpty.ps1
cargo test --locked -p alacritty_terminal
cargo test --locked
cargo run --release --locked
```

The script verifies the archive before opening it and each selected asset
before deployment. It extracts only named entries, stages the complete verified
pair, and copies it beside executables in `target/{debug,release}` and their
`deps` and `examples` directories. Download/cache files remain under `target`;
no executable binaries are committed. The cache archive is checked on every
invocation; the script repairs modified deployed assets from verified bytes.
A corrupt archive is rejected before writing executable directories. Updating
versions is an explicit manifest change requiring renewed Windows measurements.

`cargo clean` removes these generated files. Run preparation again afterward.
For custom `CARGO_TARGET_DIR` or explicit `--target` builds, supply the matching
output root. For example:

```powershell
./tools/prepare-conpty.ps1 -TargetDirectory target/x86_64-pc-windows-msvc
```

A previously downloaded pinned archive can be supplied with `-PackagePath` for
an offline restore. `-DestinationDirectories` accepts explicit output folders.
The tool uses PowerShell/.NET facilities already present on Windows; installing
a NuGet client or contacting the network from Cargo is unnecessary. Cross-checks
that only compile Rust do not need native assets, but Windows execution does.
The initial supported packaging target remains `x86_64-pc-windows-msvc`.

After `cargo build --release --locked`, run:

```powershell
./tools/package-windows.ps1
```

This first checks that the input is an x64 PE32+ executable, rejecting DLLs,
other architectures and non-PE files before creating output. It then packages
the already-built executable as `pike.exe` with `conpty.dll`,
`OpenConsole.exe`, `conpty-runtime.json` and `ConPTY-LICENSE.txt`, in a fresh
folder and ZIP. It reuses the same verification step and prints the ZIP's
SHA-256. Custom executable, target-root and output paths are parameters.
Ship that complete unit; runtime updates require replacing the matched pair.

The adapter loads only an absolute DLL path beside the running executable;
dependent DLL search is restricted to that directory and System32. It requires
the adjacent host before loading. Missing files, invalid libraries or missing
exports return an error and release any acquired module reference. It never
searches PATH or falls back to inbox ConPTY. Files in a user-selected working
directory cannot silently select another backend. Hash validation occurs during
preparation/packaging, not on every session's I/O or startup path.

The DLL is loaded when a terminal session is created, with its module reference
held until all calls, including close, have returned. There is no runtime
updater, diagnostic sampler, second PTY or extra idle task. Active process
resource measurements and limitations belong in the verification record.
