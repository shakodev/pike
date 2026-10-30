# Settings (version 1)

Pike reads **`%LOCALAPPDATA%\Pike\settings.toml`** on Windows. It does not create
the file or directory on startup. On macOS the path is
`~/Library/Application Support/Pike/settings.toml`; on other Unix systems it is
`$XDG_CONFIG_HOME/Pike/settings.toml` (or `~/.config/Pike/settings.toml`). An
unavailable/nonabsolute application-data root produces a recoverable banner.

Create/edit the UTF-8 TOML file yourself, then use **Reload settings** or
**Ctrl+Shift+F5**. The shortcut passes through the existing application resolver;
Help, tab dialogs, close decisions and active IME composition retain priority.
Bare F5 remains terminal-owned. Reload reads at most 64 KiB on one explicit
blocking job; simultaneous reloads are coalesced. There is no file watcher,
periodic read, settings background task, automatic save or settings editor.

Missing files select the defaults below, including on reload. Invalid TOML,
unknown fields/types/versions, invalid ranges and I/O failures keep the entire
last valid active configuration. Startup begins with defaults if the file cannot
be loaded. The banner gives a source line/byte column for syntax/schema errors
or a fixed field label for validation errors, without echoing values or source
lines. Correct the file and explicitly reload to clear the error.

```toml
version = 1
default_profile = "default"
theme = "dark"
history = 2000
close_policy = "confirm-running"

[font]
family = "monospace"
size = 14.0

[profiles.default]
# No shell, args, cwd or env entries selects platform defaults.
```

The Windows default shell uses the existing launch adapter's installed
PowerShell 7 → Windows PowerShell → cmd selection; WSL requires an explicit
shell. On Unix the existing adapter default is `/bin/bash`. No explicit invalid
shell silently falls back. The inherited launch directory is frozen at session
creation; this is independent of later `cd` commands inside the shell.

| Field | Accepted values / limits |
| --- | --- |
| `version` | Required integer `1`; future versions are refused. |
| `default_profile` | Identifier of a configured profile; default `default`. |
| `profiles` | 1–16 profiles; IDs use 1–32 ASCII letters/digits/`_`/`-`. |
| `profiles.<id>.shell` | Optional unquoted executable name or existing absolute file path; 1–4096 UTF-8 bytes, no controls or double quotes. Bare names use the existing platform PATH resolution at launch; final OS executable/access errors appear in the affected pane. |
| `profiles.<id>.args` | Literal argument array; explicit shell required, at most 64 entries, 4096 bytes each / 16384 bytes total, no NUL. Empty individual arguments and Unicode are preserved. Windows uses existing CRT escaping; version 1 does not expose raw command-line mode. |
| `profiles.<id>.cwd` | Optional existing absolute directory, up to 4096 bytes, no controls. No variable/tilde expansion. |
| `profiles.<id>.env` | Inherited environment plus at most 64 explicit overrides, 16384 bytes total including names/terminators. Names: 1–256 bytes, no controls, `=` or NUL; values up to 4096 bytes, no NUL. Case-insensitive duplicate ASCII names are refused. The launch adapter retains its final platform validation and inheritance behavior. No process-global mutation occurs. |
| `font.family` | `monospace`, `consolas`, `cascadia-mono`, `menlo`, `deja-vu-sans-mono`; default `monospace` prefers Consolas on Windows, generic monospace elsewhere. Named-family fallback remains Iced's system-font behavior; no fonts are bundled. The bounded static family set avoids accumulating interned names during reload. |
| `font.size` | Finite number, 10–28 DIP; default 14. Relative line height remains the adapter's 1.3; monitor DPI is managed by Iced. |
| `theme` | `dark` (default), `light`, `system`. System changes use the existing Iced OS theme event; no polling. Both terminal palette and application chrome follow the selection. |
| `history` | 0–10000 normal-screen lines per session, default 2000. Zero disables history. This is a line limit, not a strict byte cap for arbitrary cell extras. |
| `close_policy` | `confirm-running` (default) asks once for the exact targeted set containing starting/running sessions, with Cancel default; `never` commits that same cleanup path without the question. No foreground-job detection is implied. |
| `shortcuts` | Typed bounded future-remapping schema below. **R31 validates/retains it; R32 owns applying remappings.** Current shortcuts remain active. |

## What reload changes

Font family/size, palette, history and close policy apply live. Running, hidden
and exited terminal owners retain their SessionIds, widget identities, process
owners and modes. Font changes remeasure through the existing cache and resize
the same PTYs; reload resets temporary per-pane font zoom to the configured size.
Reducing history discards oldest excess history immediately; raising it permits
future growth and cannot restore discarded lines. A pending launch uses the
active appearance/history when it attaches, including after an intervening reload.
Close policy governs subsequent close requests; a decision already displayed
retains its captured session set and Cancel/Confirm behavior.

Shell, arguments, cwd and environment apply only to new/restarted processes.
Each session retains a **profile ID** separately from its frozen launch snapshot.
New tabs and splits inherit the source's profile ID, then resolve its current
configuration. A current explicit profile cwd wins; otherwise they inherit the
source's frozen launch directory. Restart resolves that same profile ID against
current settings. Choosing **New terminal with profile…** explicitly launches a
different configured profile in a new tab; the configured default is used when
there is no source terminal. The picker carries a generation so an old queued
selection cannot launch a different profile at a reused list index.

Removing a profile leaves its existing processes running. An attempt to inherit
or restart that missing profile shows a recoverable error, with no replacement
reservation or silent default. Restore/reload it, or explicitly choose a
configured profile for a new tab. **Use default shell** remains an explicit
per-error-pane recovery using the platform default and frozen launch directory;
that recovered session is detached from named-profile settings.

Two independent Windows profiles can be configured as follows (create these
directories first and use installed executable names):

```toml
version = 1
default_profile = "work"
history = 2000

[profiles.work]
shell = "pwsh.exe"
args = ["-NoLogo"]
cwd = 'C:\work'
env = { PIKE_PROFILE = "work" }

[profiles.tools]
shell = "cmd.exe"
args = ["/D", "/Q"]
cwd = 'C:\tools'
env = { PIKE_PROFILE = "tools" }
```

## Shortcut schema seam

```toml
[[shortcuts]]
command = "reload-settings"
key = "F5"
modifiers = ["ctrl", "shift"]
```

At most 64 entries; commands and canonical chords must be unique. Modifier
names are `ctrl`, `shift`, `alt`, with no duplicates. Key names are ASCII letters,
digits, `+`/`=` (one canonical key), `-`, `F1`–`F12`, `tab`, `enter`, `left`,
`right`, `up`, `down`, case-insensitive. Unsupported names and unknown schema
fields are rejected. The schema reserves application-safe chord families:
Ctrl+Shift keys except terminal copy/paste C/V; Alt or Alt+Shift arrows; Ctrl+Tab;
bare F1. Ctrl+C, AltGr text and unmodified typing cannot be remapped through it.
This schema is data for the common resolver's future R32 configuration, with no
additional key dispatcher in R31.

Typed command names are `new-tab`, `close-pane`, `close-tab`, `next-tab`,
`previous-tab`, `tab1`–`tab8`, `last-tab`, `split-right`, `split-down`,
`focus-left`/`right`/`up`/`down`, `resize-left`/`right`/`up`/`down`, `toggle-zoom`,
`font-larger`, `font-smaller`, `font-reset`, `help`, `rename-tab`, `list-tabs`,
`reload-settings`.

## Explicit saving API

`settings::Store::save(&Settings)` is available to callers that explicitly save
validated settings; the application does not call it automatically or expose a
save UI. It refuses an invalid existing target, including an invalid file edited
during preparation. Correct or move the broken file yourself first.

Saving creates a unique file in the same directory with `create_new`, writes a
complete bounded document, flushes it with `sync_all`, closes its handle, then
replaces the target using `std::fs::rename`. On Windows this uses
[MoveFileExW replacement / FileRenameInfoEx fallback](https://doc.rust-lang.org/std/fs/fn.rename.html);
[Windows replacement](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw)
requires the appropriate ACL and sharing permissions. It never deletes the
target or falls back to copy/delete. A failed write/flush/replace preserves the
previous target and cleans up only its own temporary file. A process interrupted
before replacement can leave a `.pike-settings-*.tmp`; startup ignores such
files. Atomic publication does not promise recovery from arbitrary filesystem
or power-loss corruption, or synchronization with an editor writing concurrently.
