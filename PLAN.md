# Pike application plan

Planning baseline: 2026-10-25. Status: researched design; **no application implementation has been started**.

## 1. Objective and scope

Build a native, Windows-first terminal multiplexer with reliable independent shells, fast splits, tabs, and a compact interface. Correct input, output, resize, and shutdown behavior take priority over visual effects. Idle work should approach zero. Use stable Rust, Iced 0.14 with `pane_grid`, and iced_term 0.8 backed by Alacritty/ConPTY.

Orca supplies the visual reference: restrained chrome, clearly focused panes, nested splits, and compact tab navigation. Pike's MVP deliberately uses **one tab containing one split tree**, with one independent session per leaf. This is simpler than Orca's tab groups inside panes. It has no editor, browser, agent orchestration, workspace daemon, or remote-session service. Reference: [Orca panes and tabs](https://www.onorca.dev/docs/model/tabs-panes-splits).

MVP release = phases P0–P5. P6 consists of separately scoped, optional enhancements after the MVP passes its correctness and performance gates. Background session persistence after application exit, image protocols, plugins, built-in SSH, cloud sync, broadcast input, GPU shaders, and a second PTY implementation are outside this roadmap.

### Repository and verification baseline

- Local and GitHub repository: [shakodev/pike](https://github.com/shakodev/pike), branch `main`, initial commit `63d5926`.
- Tracked file at inspection: `.gitignore`. User-provided `AGENTS.md` is present and initially untracked; preserve it.
- No `Cargo.toml`, `Cargo.lock`, Rust source, executable, CI, tests, or existing knowledge graph exists. There are **no pinned project dependencies to verify yet**.
- Local compiler: `rustc 1.97.1 (8bab26f4f 2026-07-14)`; Cargo 1.97.1; active `stable-x86_64-pc-windows-msvc`.
- None of the relevant crates were installed in the local Cargo source cache. Exact published crate archives and tagged upstream examples were downloaded into a temporary research folder and inspected as source.
- Initial GitHub audit found zero issues and zero milestones. Existing labels were inspected before creating this roadmap.
- No application benchmarks or Windows behavior results exist. Every budget below is a **proposed acceptance target**, not a measurement. Compiling/running prototypes belongs to the first implementation issues.

## 2. Verified dependency and API findings

### Version baseline and reproducibility

| Component | Verified source | Decision |
|---|---|---|
| Iced | Published 0.14.0, edition 2024, declared Rust minimum 1.88; tag commit `3997291f318a8bc06fa522f5579836fb3feb94df` | Start with exact `=0.14.0`; use versioned sources, not the 0.15 development documentation. |
| iced_term | Published 0.8.0, edition 2021, no declared Rust minimum; tag/VCS commit `e0e90be2c160bd68f1bf544a51bf772d75b98ffc` | Start from exact 0.8.0 and maintain a small, documented patch set. |
| Alacritty terminal | iced_term declares `0.25.1`; published source declares Rust 1.85 | Keep 0.25.1 as the initial tested baseline; upgrading to 0.26 is a separate compatibility decision. |
| Upstream lockfile | iced_term's archive records Iced 0.14.0, Alacritty 0.25.1, Tokio 1.50.0, winit 0.30.13, wgpu 27.0.1 | This is evidence, **not Pike's lockfile**. Resolve Pike's complete graph and commit its own lockfile in R01. |
| Rust toolchain | Installed stable 1.97.1 exceeds Iced's declared minimum | Pin the initially verified compiler in `rust-toolchain.toml`; do not claim whole-graph MSRV 1.88 without testing it. |

Cargo versions such as `"0.8.0"` are compatible version ranges, not exact pins. Commit `Cargo.lock`, use `--locked` in CI, review updates explicitly, and retain provenance for patches. A dependency's packaged lockfile does not control its consumer.

**Feature unification is a material resource risk.** iced_term 0.8.0 enables Iced defaults plus `canvas`, `lazy`, and `advanced`; it also enables Tokio `full`. Iced defaults include both wgpu and tiny-skia, its thread pool, and platform features. Setting `default-features = false` only in Pike does not disable features requested by iced_term. Its manifest must be patched to make the lean profile possible. Confirm the actual graph with `cargo tree -e features` and `cargo tree -d`; a feature list alone is not a RAM measurement. [Iced manifest](https://docs.rs/crate/iced/0.14.0/source/Cargo.toml), [iced_term release manifest](https://github.com/kemokempo/iced_term/blob/0.8.0/Cargo.toml).

### APIs that can be used as published

| Area | Verified 0.14/0.8 API | Consequence |
|---|---|---|
| Application | `iced::application(boot, update, view)`, `.subscription(...)`, `.executor::<E>()`, `.run()` | Use the 0.14 functional application API; older `Application` trait examples are inappropriate. |
| Layout | `pane_grid::State<T>::new`, `split`, `resize`, `adjacent`, `drop`, `swap`, `maximize`, `restore`, `close`, `layout` | Store lightweight pane state in the grid; use its existing tree. |
| Widget | `PaneGrid::new`, `.on_click`, `.on_drag`, `.on_resize(leeway, ...)`, `.min_size` | Built-in layout interactions suffice; no custom tiling engine. |
| Split axes | `Axis::Vertical` yields left/right; `Axis::Horizontal` yields top/bottom | UI actions are named **Split right** and **Split down**, avoiding ambiguous orientation terminology. |
| Last pane | `State::close` returns `None` when no sibling can replace the root | Last-pane and last-tab policies must be handled at application level. |
| Terminal creation | `Terminal::new(u64, Settings) -> std::io::Result<Terminal>` | Creation is fallible and synchronous; background backend construction needs a narrow extension. |
| Session configuration | `BackendSettings { program, args, env, working_directory }` | Pass structured launch settings. Default on Windows is **wsl.exe**, so override it. |
| View/focus | `TerminalView::show(&Terminal)`; `TerminalView::focus(terminal.widget_id().clone())` | Focus requires both application state and widget focus synchronization. |
| Subscription | `Terminal::subscription()`; events are `Event::BackendCall(id, BackendCommand)` | Subscribe to all live sessions, including hidden ones. Route by event session ID. |
| Commands | `Terminal::handle(Command::ProxyToBackend(...)) -> Action`; public `BackendCommand::{Write, Scroll, Resize, SelectStart, SelectUpdate, MouseReport, ProcessAlacrittyEvent, ProcessLink}` | Backend internals are private; missing hooks require a patch, not access to undocumented fields. |
| Actions | `Action::{Shutdown, ChangeTitle(String), Ignore}` | Exit codes, bell, title reset, protocol queries, and teardown acknowledgement need explicit integration. |
| Settings | `Command::{ChangeFont, ChangeTheme, AddBindings}` | Font/theme changes exist; no public history cap, selection extraction, mode getter, or shutdown completion API exists in 0.8.0. |
| Window close | `.exit_on_close_request(false)`, `window::close_requests()`, `window::close(id)` | Keep the window alive until session cleanup completes or the user chooses an explicit timeout action. |
| Keyboard subscription | `keyboard::listen()` receives ignored keys; `event::listen_with` observes handled events too | Neither is a pre-dispatch shortcut interceptor. A shortcut must be consumed before terminal input is generated. |

Reviewed examples: Iced [pane_grid](https://github.com/iced-rs/iced/blob/0.14.0/examples/pane_grid/src/main.rs); iced_term [full_screen](https://github.com/kemokempo/iced_term/blob/0.8.0/examples/full_screen/src/main.rs), [split_view](https://github.com/kemokempo/iced_term/blob/0.8.0/examples/split_view/src/main.rs), [focus](https://github.com/kemokempo/iced_term/blob/0.8.0/examples/focus/src/main.rs), and the custom-bindings/font settings APIs. The split example incorrectly handles a session's shutdown by closing the **currently focused pane**. Pike must locate the originating session instead.

### Source-level limitations and required treatment

The findings below are verified from published source unless marked as an inference or runtime risk. Proposed extension names are architectural contracts, **not claims that upstream exposes them**.

| ID | Evidence / limitation | Planned treatment | Owner |
|---|---|---|---|
| F01 | EventProxy uses `blocking_send` into a 100-event Tokio queue. Backend operations take the terminal mutex. Upstream acknowledges GUI deadlock in PR 69. | Nonblocking producer, coalesced dirty state, reliable exit/failure state, bounded control traffic, explicit overload policy. Never wait for UI queue capacity while holding the terminal lock. | R05 |
| F02 | Subscription adds a 1,000-message channel, panics on output send failure, and loops after channel closure rather than terminating. | Cancellation/EOF must finish the stream; closed channels cannot spin or panic. Remove redundant buffering if measurements justify it. | R05 |
| F03 | `Backend::internal_sync` clones the **entire grid including history** on every `Terminal::handle`. Default history is 10,000 lines and is not configurable through iced_term. | Expose capped history; snapshot visible cells and selection metadata only. Hidden sessions keep parsing but skip snapshots. | R11 |
| F04 | Every handle call remeasures font, issues resize for positive dimensions even if unchanged, synchronizes, and clears canvas cache. | Cache metrics; separate mutation, snapshot, geometry invalidation, and resize. Send only changed cell dimensions. | R12 |
| F05 | Widget state has no terminal identity in `diff`; cached geometry uses absolute layout coordinates. | Backport identity fix, explicitly invalidate on movement/DPI/size, clip each pane; test same-sized tab switches and moves. | R13 |
| F06 | Drop sends `Msg::Shutdown`, but spawned PTY JoinHandle is discarded. `EventLoop::new(..., false, false)` disables drain-on-exit. | Request-close/completion protocol, output drain, off-UI cleanup, exit code retention, final snapshot, idempotent closure. | R07 |
| F07 | Windows ConPTY creation asserts success; raw handles/attribute-list ownership and failure paths need review. `ChildExitWatcher::drop` only unregisters its wait. | Audit every handle and failure edge before claiming leak-free cleanup. Reproduce and patch proven failures in the existing backend. These are source concerns, not measured leaks. | R08 |
| F08 | Windows launch leaves `escape_args` false; command-line construction appends program text directly. | Verified executable quoting and structured argument serialization, with separate tests for PowerShell and cmd semantics. Do not concatenate untrusted command strings. | R09 |
| F09 | Selection copy concatenates visible cell characters without proper line/wrap handling. Paste sends raw clipboard bytes. | Use Alacritty `selection_to_string`; add mode-aware bracketed paste, explicit newline policy, size limits, and admission control. | R15, R16 |
| F10 | Ctrl+U is encoded incorrectly; modifier state and AltGr/dead keys need validation; no kitty keyboard encoder. | Correct legacy key table, distinguish text/IME/shortcuts, keep kitty disabled until a complete encoder exists. | R14, R19, R45 |
| F11 | Mouse handling covers only part of expected tracking; pixel/line scroll direction differs; release outside pane is fragile. | Correct SGR/legacy tracking, shift-selection escape, pointer release, alternate-screen behavior, and scroll accumulation. | R17 |
| F12 | Per-cell rendering uses only `cell.c`, omitting combining data; full-width placement is incomplete. Cursor/background draw ordering can hide the cursor. | Wide/combining glyph tests and rendering fixes; explicit cursor shape/visibility/render order; no blinking by default. | R18, R21 |
| F13 | Only Exit, Title, and PtyWrite receive explicit backend handling. Color/size queries, ResetTitle, Bell, ChildExit, focus and cursor state require handling. | Exhaustive event policy and fixture tests; respond without unnecessary snapshots. OSC52 disabled by default. | R20, R21 |
| F14 | Alacritty uses an unbounded message/write backlog. Windows adds reader/writer workers and 1 MiB pipe buffers, besides the PTY loop's buffer. | Bound admitted input bytes end-to-end, expose drained-byte accounting if necessary, count actual workers/memory. Do not add another PTY. | R06, R38 |
| F15 | Hyperlink opening invokes `open::that` and panics on failure; many schemes match the built-in regex. | Disable built-in link opening in MVP. Later use explicit user activation, supported schemes, and recoverable errors. | R14, R44 |

Primary source paths: [terminal.rs](https://github.com/kemokempo/iced_term/blob/0.8.0/src/terminal.rs), [backend.rs](https://github.com/kemokempo/iced_term/blob/0.8.0/src/backend.rs), [view.rs](https://github.com/kemokempo/iced_term/blob/0.8.0/src/view.rs), [settings.rs](https://github.com/kemokempo/iced_term/blob/0.8.0/src/settings.rs), [bindings.rs](https://github.com/kemokempo/iced_term/blob/0.8.0/src/bindings.rs), [font.rs](https://github.com/kemokempo/iced_term/blob/0.8.0/src/font.rs); [Alacritty 0.25.1 source](https://docs.rs/crate/alacritty_terminal/0.25.1/source/src/).

### Available upstream fixes, checked 2026-10-25

Backport small fixes after reviewing their exact diff and regression tests; do not switch to moving master. Current master has also moved dependencies beyond the requested baseline.

- Merged after 0.8.0: [69 event delivery](https://github.com/kemokempo/iced_term/pull/69), [70 widget identity](https://github.com/kemokempo/iced_term/pull/70), [71 scroll direction](https://github.com/kemokempo/iced_term/pull/71), [85 mouse drag](https://github.com/kemokempo/iced_term/pull/85), [86 cursor contrast](https://github.com/kemokempo/iced_term/pull/86), [89 Ctrl+U](https://github.com/kemokempo/iced_term/pull/89), [90 copy](https://github.com/kemokempo/iced_term/pull/90), [91 bracketed paste](https://github.com/kemokempo/iced_term/pull/91).
- PR 69 only replaces blocking send with `try_send` and ignores failure. It helps deadlock risk but can lose critical events; it is insufficient on its own.
- [73 IME/wide glyphs](https://github.com/kemokempo/iced_term/pull/73) remains open; treat it as reference material requiring independent verification.
- [100 Micro cursor visibility](https://github.com/kemokempo/iced_term/issues/100) remains open. Source findings agree that draw order deserves a regression test.
- [58 asynchronous shutdown](https://github.com/kemokempo/iced_term/pull/58) was closed; no published API can be assumed.
- [96 text extraction](https://github.com/kemokempo/iced_term/pull/96) is merged after the release; potentially useful for later search, but not part of 0.8.0.

### Patch policy

Start with `vendor/iced_term` containing the exact 0.8.0 crate, its MIT license, source checksum, upstream revision, and a short patch ledger; wire it using `[patch.crates-io]`. This is planned work in R04, not content added by this planning task. Prefer small upstreamable patches. An Alacritty patch is permitted only for demonstrated resource, launch, or input-budget gaps (R06/R08/R09), retaining its existing PTY/emulator.

Do not expose the whole backend publicly or replace rendering wholesale merely to reach private fields. Add the smallest capabilities needed: backend prepare/attach, lifecycle completion, capped history, viewport snapshot, correct selection/paste, reliable event traffic, input acknowledgements, and visibility-aware refresh. Each extension needs a focused test and a ledger entry identifying its future upstream removal condition.

## 3. Product specification

### MVP behavior

| Feature | User-facing behavior and edge cases |
|---|---|
| First launch | One tab, one shell, keyboard focus ready. Choose configured shell; otherwise installed PowerShell 7, Windows PowerShell, then cmd. WSL is explicit opt-in. A missing/invalid configured shell produces an error pane with Retry/Choose default, never a silent switch. |
| Working directory | An explicitly supplied directory must be valid; otherwise show a launch error. If none was supplied, use the valid inherited launch directory, then user home. New tabs/splits inherit the source **launch** directory/profile. Do not claim to know a shell's current directory without shell integration. |
| Splits | Split right/down creates a new independent process. Initial ratio 0.5. Existing sessions and buffers remain intact. Reject a split when geometry or session limit would make it unusable. Failure leaves the original pane usable and a removable error placeholder. |
| Resizing | Drag divider or use keyboard; clamp to safe geometry. Preserve ratios when the window changes size. Update PTY rows/columns only when changed; do not send zero sizes on minimize. |
| Focus | Click content/header or use directional focus. Exactly one terminal receives keyboard input; menus/dialogs temporarily own it. Return focus to the prior terminal when dismissed. |
| Move/zoom | Header-only dragging rearranges panes within the same tab. Content drag selects text. Maximize/restore preserves the tree and every process; hidden sessions continue reading. |
| Tabs | Ordered tabs each contain a split tree. New, switch, rename, reorder, close; next/previous and direct selection shortcuts. Switching preserves terminal modes, scrollback, selection, size knowledge, and last focused pane. |
| Close | Close pane affects only its session. Closing a tab/window asks once if any targeted session is starting/running, with count and Cancel as default. This conservative policy does not pretend to detect foreground jobs. Exited/error panes close without confirmation. |
| Last pane/tab | Last pane closes its tab. Last tab leaves a lightweight empty window with New terminal; explicit window close exits. No automatic shell respawn loop. |
| Natural exit | Retain read-only final output plus exit code if available; mark Exited, offer Restart/Close. Release PTY/workers promptly. Restart allocates a new SessionId, so late old events are harmless. |
| Copy/selection | Drag, double-click word, triple-click line; preserve hard newlines and soft-wrap semantics; copy from scrolled history and Unicode correctly. Ctrl+C remains interrupt. Shift overrides TUI mouse tracking for selection. |
| Paste | Ctrl+Shift+V / Shift+Insert; no implicit Enter. Bracketed-paste markers only in that mode; filter marker-breaking controls consistently with Alacritty. Handle CRLF and normal-mode newline conversion explicitly. Clipboard unavailable/oversized gives a compact actionable error. |
| Scrollback | Default 2,000 lines/session, configurable 0–10,000. Bound supported columns too (initial maximum 512); no duplicate transcript. Wheel targets hovered pane without changing focus. Shift+PageUp/Down and Shift+Home/End navigate history. Output does not pull a scrolled viewport to bottom; typing does. |
| Terminal compatibility | VT/ANSI colors, cursor control, normal/alternate screen, application cursor mode, legacy keyboard/mouse, bracketed paste, Unicode width/combining text, and Windows IME. Test cmd, PowerShell/PSReadLine, WSL when available, and full-screen TUIs. |
| Font/theme | System monospace fallback (prefer Consolas on Windows), readable 14 DIP starting size, 10–28 DIP zoom, dark/light/system palettes. No bundled Nerd Font by default. Font changes resize existing PTYs without restarting. |
| Configuration | Small versioned local file for shell, args, cwd, font, palette, history cap, close confirmation, and shortcuts. Load once, explicit reload; no watcher or background settings UI. Invalid configuration shows a useful error and retains last valid settings. |
| Titles/activity | User rename wins over bounded sanitized OSC title; shell/profile fallback. One unread/bell indicator per background tab, not a per-line counter. Clear when visited. No terminal text in diagnostics. |
| Error recovery | In-pane error banner; retry allocates a fresh session. No panic for routine spawn/clipboard/open/resize/close errors. No global crash when another session fails. |

Resource guardrails for MVP: maximum 32 retained session entries per window (Starting, Running, Closing, Exited, or Failed), default 2,000 history lines, validated 512-column maximum, title cap 256 Unicode scalar values, and bounded diagnostic records. Closing entries consume a slot until cleanup is acknowledged; exited/error panes consume a slot until closed. Initial accepted paste limit 1 MiB and total outstanding user-input budget 2 MiB/session; these are tunable engineering defaults, not truncation permissions. Reject oversize paste **before writing any of it**. Queue saturation must be visible, preserve accepted-byte order, and keep close/cancel responsive. Line limits alone are not a strict byte cap for arbitrary combining text; stress tests must quantify that residual risk.

### Shortcuts and input priority

Defaults use Ctrl+Shift where possible to leave ordinary shell controls available. All app bindings can be remapped or disabled.

| Action | Default |
|---|---|
| New tab / close pane / close tab | Ctrl+Shift+T / Ctrl+Shift+W / Ctrl+Shift+Q |
| Next / previous tab | Ctrl+Tab / Ctrl+Shift+Tab |
| Select tab 1–8 / last tab | Ctrl+Shift+1…8 / Ctrl+Shift+9 |
| Split right / down | Ctrl+Shift+O / Ctrl+Shift+E |
| Focus adjacent pane | Alt+Arrow |
| Resize toward direction | Alt+Shift+Arrow |
| Maximize / restore pane | Ctrl+Shift+Enter |
| Copy / paste | Ctrl+Shift+C / Ctrl+Shift+V; Shift+Insert pastes |
| History navigation | Shift+PageUp/Down; Shift+Home/End |
| Font larger / smaller / reset | Ctrl+Shift+Plus / Ctrl+Shift+Minus / Ctrl+Shift+0 |
| Rename tab / all tabs / shortcut help | Ctrl+Shift+R / Ctrl+Shift+L / F1; bare F2 remains terminal-owned |
| Interrupt shell | Ctrl+C passes through unchanged |

Routing order: active modal/IME composition rules → recognized application shortcut → focused terminal encoder. App shortcuts are captured **before** child widgets receive them, using a small advanced-widget wrapper or a narrow iced_term hook. Do not depend on post-dispatch subscriptions: a key can otherwise both split a pane and reach the shell. Do not use `BindingAction::Ignore` to suppress input: the 0.8.0 character path can fall through to text. Modifier changes must remain synchronized, repeated create/close keys are suppressed, and AltGr must remain printable on non-US layouts. Window focus loss resets transient modifiers and drag/composition state appropriately.

### UI design

Use native Windows decorations for resizing, snapping, caption buttons, and system behavior. Inside: a 32–36 DIP tab strip with a plus button and overflow list, a compact 24–28 DIP pane header when split, and terminal content filling the rest. Use a 1 DIP focused border, 4 DIP visual gutters with an approximately 8 DIP drag target, restrained neutral surfaces, and one accent. Target at least 80×24 cells on initial window size where screen geometry permits.

Pane minimum is based on cells (target 20×5 plus header/padding), not only a constant pixel square. If a window becomes smaller than the current tree can support, keep positive backend dimensions and clipped/pannable-safe presentation with a visible size constraint; never destroy panes to fit. Resolve the exact small-window treatment in R25 with screenshots.

Show split controls for the focused pane, close controls on hover and keyboard focus, tooltips with shortcut names, and persistent focus indicators. Avoid animated spinners, transitions, blur, acrylic, per-pane shadows, or permanent CPU/RAM displays. Running status is event-driven. An error banner can take space; it must not obscure the prompt or steal typing focus.

Keyboard navigation and high-contrast chrome are MVP requirements. The canvas terminal's screen-reader text exposure is an identified platform limitation; do not claim full accessibility. R35 measures and documents it, while R46 defines a separately gated implementation path.

## 4. Architecture and ownership

Use one application crate initially. Modules are boundaries, not a requirement for dozens of files before functionality exists.

| Module | Responsibility | Must not own |
|---|---|---|
| `main.rs` | Boot, selected executor, window settings, fatal startup result | Layout/session business rules |
| `app.rs` | Top-level state, update orchestration, view/subscription composition | Terminal parser or native PTY |
| `workspace.rs` | Ordered TabIds, tab labels, active tab, atomic layout commands | Terminal instances |
| `layout.rs` | `pane_grid::State<PaneState>`, focus, ratios, move/zoom, min geometry | OS processes |
| `session.rs` | Registry, SessionId allocation, launch/exit/close states, lifecycle effects | Pane coordinates |
| `terminal.rs` | Small iced_term adapter, event policy, render visibility, typed terminal commands | Tab ordering |
| `input.rs` | Commands, shortcut resolution, focus ownership, modal routing | Emulator state |
| `settings.rs` | Validated settings and launch profiles, safe load/save | Live terminal text |
| `ui/` | Tab strip, pane chrome, empty/error/confirmation/help views, palette tokens | Process lifetime |
| `diagnostics.rs` | Optional counters and bounded redacted error metadata | Terminal transcripts or secrets |
| `tests/`, `tools/` | Synthetic VT fixtures, Windows integration helper, benchmark scripts | Runtime monitoring services |

Start with std collections and enums. Add `serde` and one small configuration format crate (TOML) only in R31. Reuse existing Iced/Tokio facilities for commands, channels, timers, and clipboard. Do not add an ECS, actor framework, event bus library, dependency injection container, UUID crate, global shared app mutex, database, analytics, or sysinfo runtime dependency.

### State and invariants

- `App`: workspace, session registry, validated settings, overlay/focus target, application-close state.
- `Workspace`: `Vec<TabId>`, tab map, active tab. A tab contains `pane_grid::State<PaneState>`, focused pane, optional manual title; maximize state comes from the grid.
- `PaneState`: SessionId and minimal presentation metadata. SessionId is never derived from `Pane`, an array position, PID, or tab index.
- `SessionId(u64)` and `TabId(u64)`: monotonically allocated for the process lifetime; checked overflow. Never reuse a closed session ID. Iced's subscription identity uses the u64 ID.
- `SessionEntry`: launch spec, lifecycle state, terminal handle or spawn ticket, last valid dimensions, title, exit information, visibility/dirty flags, optional bounded error. Exactly one terminal emulator and one PTY per live session.
- Lifecycle: Starting → Running → Closing → Closed, or Running → Exited; startup can become Failed. Exited retains a read-only bounded terminal buffer without a live PTY. Closed entries are removed after completion. A failed/late spawn result must still be closed.
- Every pane references exactly one existing entry; each displayed entry has exactly one pane. Starting/Closing entries may temporarily live without a pane while a transaction settles.
- UI messages identify TabId + pane for layout actions, and SessionId for backend actions. Validate both before using stale messages.
- Layout edits do not drop/recreate Terminal. Closing is the only intentional lifetime change, apart from restart/natural exit.
- Hidden tabs and maximized-away panes remain subscribed while live. A closed/failed stream terminates and is removed; an exited viewport can still be selected/copied.
- App state mutates on Iced's update thread. Workers return typed results. A testable update layer can return a small `Effect` enum for spawn, close, focus, clipboard, and settings I/O; it does not need a generic effect framework.

### Main data flow

```text
Mouse / keys ──> focus + shortcut routing ──> layout commands or terminal input
                        │                              │
                        v                              v
              Workspace / pane_grid        SessionRegistry[SessionId]
                                                       │
                                          narrow iced_term adapter
                                                       │
                                     existing Alacritty PTY event loop
                                                       │
                                               Windows ConPTY
                                                       │
                  UI <── bounded control + dirty notification <── shell
                   └── visible viewport snapshot ──> cached Iced rendering
```

### Lifecycle transactions

**Create/split:** validate profile, dimensions, and session budget; reserve a fresh SessionId and show a Starting placeholder; prepare the backend on bounded blocking work; attach UI font/cache state on the UI thread. Commit only if the target/request still exists. If canceled or moved while starting, resolve by request ID and dispose of unused prepared resources. A failed split never closes the source.

**Natural exit:** record ChildExit when available, drain remaining output, obtain final snapshot, and detach/join backend resources off the UI thread. Keep the terminal model for read-only scroll/copy. Failure of the I/O loop without an Exit event still needs a completion/error notification. Do not infer success from an absent code.

**Close:** resolve the precise target SessionIds; confirm once if required; mark Closing, disable writes, and issue idempotent shutdown. Continue draining relevant control/output and complete cleanup asynchronously. If a pane is removed immediately, the registry still owns its closing session until acknowledgement. Last-pane removal is handled by deleting the tab rather than relying on `pane_grid::close`.

**Application close:** intercept window-close requests, reject new spawns, initiate all requested closes, wait asynchronously, then close the window. Initial timeout policy: after 3 seconds show remaining session count and Wait/Force close. Never block the UI joining threads. Force close is scoped to owned resources; no broad process-name killing or arbitrary PID reuse. Attached children are expected to close with ConPTY; intentionally detached descendants are not promised to be terminated. A Windows Job Object is a conditional remedy only if measured lifecycle tests demonstrate a need.

**Windows constraint:** ConPTY cleanup is version-sensitive. Microsoft documents that pre-Windows 11 24H2 close can wait indefinitely if output is not drained, while 24H2 returns sooner. Keep backend I/O alive through teardown and verify both supported test cohorts. Source evidence: [ClosePseudoConsole](https://learn.microsoft.com/en-us/windows/console/closepseudoconsole), [session creation/cleanup](https://learn.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session).

### Concurrency and flow control

Use one Iced executor for the whole application. Planned lean configuration: a small `iced::Executor` adapter around one Tokio runtime, initially one async worker and at most two blocking workers; enable only required runtime/time/sync features. This is supported by the inspected Executor trait and `.executor::<E>()`; preserve `enter`, `spawn`, and `block_on` behavior. R03/R04 compare overhead and adjust only from evidence. There is never one async runtime per pane.

Retain Alacritty's event-driven PTY loop and existing Windows pipe workers. Expect **at least three backend workers per live Windows session**, plus runtime/renderer/OS overhead; count them rather than assuming one thread per terminal. Spawn/cleanup are bounded temporary work; admission control prevents a queue of unlimited spawn tasks.

The event bridge has two classes:

1. Replaceable state: dirty/wakeup, latest title, latest dimensions, and bell flag coalesce into bounded state plus a wake notification.
2. Ordered control: protocol responses and input retain ordering; completion/exit/error have reliable sticky state and cannot be discarded by a full redraw queue.

Proposed control capacity is 256 entries with payload accounting; exact capacity is verified under load. Producer callbacks must never block while holding Alacritty's terminal lock. On saturation, apply backpressure at a safe parser/IO boundary if supported; if that cannot be achieved with a small patch, transition the affected session to an explicit overload error while preserving exit/completion delivery. Do not silently lose protocol replies or use unbounded fallback queues. This is a concrete spike and regression gate in R05, not an assumption that a one-line `try_send` substitution is enough.

Input has a byte budget, not just a message count. Chunk accepted large pastes; account bytes until the existing PTY writer consumes or rejects them. Do not reset the budget merely after enqueueing to Alacritty's unbounded queue. R06 may require a narrow consumption acknowledgement in that backend. A blocked child cannot cause unlimited memory growth, key reordering, or deadlock during close.

### Rendering, sizing, and idle behavior

- Keep the authoritative Alacritty grid once; no application-owned terminal transcript. A render snapshot copies only the visible viewport plus required metadata, under a short terminal lock, then draws without holding it.
- Hidden sessions parse output and process protocol/lifecycle events, but build no geometry or viewport snapshots. Mark them dirty; on activation perform one current snapshot.
- Coalesce repeated visible invalidations to display frames under load; the first update should remain responsive. Schedule a one-shot follow-up only while dirty; no permanent 60 Hz subscription.
- Blink is off by default. Optional later/on-request blinking runs only for a visible focused cursor, pauses on window blur/minimize, and must not remeasure fonts or clone history.
- Deduplicate font metrics and resize by actual cell dimensions. Treat FontSettings.scale_factor as line-height-related (per source), **not** as the Windows monitor DPI multiplier.
- Compute terminal bounds after header/padding; use Iced logical coordinates, validated cell metrics, and the monitor scale exactly once. Bound rows/columns to valid ConPTY limits; preserve last good nonzero dimensions while hidden/minimized.
- Invalidate cached geometry on terminal data/selection/cursor, viewport origin/size, font, DPI, theme, and movement. Use clipping to prevent adjacent-pane bleed.
- Renderer decision remains a measured gate: compare tiny-skia-only and wgpu-only after removing forced defaults. Prefer the lower idle-memory option that meets correctness, text quality, resize, and output-latency budgets. If GPU wins, select only the required Windows backend features. Ship one default renderer initially; retain a fallback only if tested hardware needs it.

## 5. Errors, configuration, and privacy

Recoverable error categories: invalid settings, invalid executable/cwd/arguments, spawn failure, backend I/O failure, protocol queue overload, clipboard failure, unsupported launch environment, renderer initialization, and cleanup timeout. Preserve the originating SessionId and operation, show concise user text, and include OS error codes where safe. Unexpected programming invariants can fail tests; expected runtime failures cannot use `unwrap`/`expect`.

Terminal titles are untrusted display strings: strip control characters, bound length, render as plain text, and never use them as file paths or commands. Do not log raw `Event` Debug output: Alacritty events can contain clipboard or terminal text. Diagnostics are off by default; opt-in counters/errors record IDs, durations, dimensions, queue counts, and error kinds only. Do not log input, clipboard, output, full environment, or full shell arguments. Validate Windows environment keys case-insensitively before invoking upstream code that could log duplicate values.

Configuration lives under a documented per-user Windows application-data directory (no portable-mode assumption until packaging). Schema version 1; atomic replace with a same-directory temporary file and recoverable failure; validate size/ranges/duplicate shortcuts. A broken file is not overwritten automatically. Environment overrides are inherited plus explicit user changes, never process-global mutation for individual sessions. Settings affecting new processes apply to new/restarted sessions; palette/font/history changes have explicit live behavior.

## 6. Verification and acceptance targets

### Test layers

1. **Pure state tests:** generated deterministic command sequences over tabs/panes/session IDs; reference integrity, exactly-once close, stale events, canceled spawns, no lifecycle effects from move/resize/zoom. Standard tests suffice initially; add property-test dependencies only if coverage needs them.
2. **Adapter regression tests:** bounded bridge saturation/EOF/cancellation, final output, byte budgets, key byte fixtures, clipboard modes, selection wraps, queries, wide/combining cells, terminal identity, font and size invalidation.
3. **Windows integration harness:** small deterministic child helper for echo, delayed reads, controlled exit codes, final output, child/grandchild lifetimes, and terminal-size reports. Use Alacritty/ConPTY only. Generate synthetic content; never record the user's terminal.
4. **GUI/manual checks:** focus, menus, copy, IME, DPI, selection, drag/resize, clipping, alt-screen TUIs, and keyboard layouts. Screenshots and input traces use fixtures. Test hooks must not become a production polling subsystem.
5. **Release stress/performance:** reproducible process counters, frame/input timings, repeated open/close, overload and scrollback saturation; include app and child costs separately.
6. **CI:** Windows stable fmt/clippy/test with lockfile, release build, lightweight adapter/integration smoke tests. Hardware-sensitive budgets run on a designated reference machine; shared CI gets trend reporting rather than fragile absolute thresholds.

Required commands for implementation PRs:

```text
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release --locked
```

CI should also use locked dependency resolution for clippy/test. Vendor patches need their relevant upstream tests run explicitly: application tests do not automatically execute dependency tests.

### Windows support and compatibility matrix

Primary release target: x86_64 MSVC on Windows 11 24H2 or later. Compatibility cohort: Windows 10 22H2 / Windows 11 before 24H2, subject to an available test machine; do not advertise an untested OS. The ConPTY API minimum is older than the supported product baseline. ARM64 and Linux/macOS packaging are not MVP promises.

Test 100/125/150/200% scaling, mixed-DPI moves, minimize/restore, maximize, display disconnect, sleep/resume, RDP or software-rendering conditions when available. Test cmd, Windows PowerShell, PowerShell 7 with PSReadLine, and optional WSL bash; programs absent from a machine must be marked skipped, not passed. Exercise vim/neovim, Micro cursor reproduction, a pager, progress/color output, mouse TUI, and a synthetic raw byte/VT helper. Test US plus AltGr keyboard layout and at least one Windows CJK IME.

### Benchmark protocol

Record OS/build, CPU/core count, RAM, GPU/driver, monitor scale/refresh, renderer/features, compiler, build hash, shell/version/args, pane dimensions, history cap/fill, and scenario. Use optimized release builds outside a debugger. Report median/p95, sample count, raw CSV, and method; record power mode. Do not confuse private bytes with working set or compare debug with release.

Scenarios: 1/4/8 visible sessions; eight hidden but live sessions; empty history and saturated 2,000-line history at 120 columns; 32-session guardrail; 60 seconds idle after 10 seconds settling; ten-minute sustained output; 100 and 500 create/close cycles; rapid resize while producing output; stalled-reader paste; tab switching under background flood. Use a deterministic helper so shell/user profile startup is measured separately.

Measure Pike private bytes and working set, child/ConPTY process costs, CPU time normalized to **one core** (`100 × delta CPU seconds / wall seconds`), thread/handle counts, redraw/snapshot counts, input-to-render latency, split-to-ready and tab-switch latency, and PTY throughput. Sum private bytes for process trees, not shared working sets; report WSL VM memory separately. Benchmark scripts may sample externally; production does not.

| Metric | Initial target / gate |
|---|---|
| Idle Pike CPU, blink off | ≤0.1% of one core for one session; ≤0.5% for eight, over 60 seconds; no periodic app timer. |
| Idle app redraw/snapshot work | Zero app-requested continuous redraws after settling; no hidden snapshots. Explain OS-triggered paints separately. |
| Pike private bytes, empty history | ≤80 MiB one session, ≤140 MiB four, ≤220 MiB eight at controlled geometry. |
| Added saturated history | ≤25 MiB per 120-column/2,000-line session beyond empty baseline; plateau after history limit. |
| Memory recovery | After 500 close cycles and settling, private bytes ≤10 MiB above warmed baseline, no linear growth; worker/handle counts return to expected baseline within five seconds. |
| Input latency | p95 ≤50 ms under four simultaneous 2 MiB/s synthetic output producers; 30-second run, ≥200 samples. Report actual delivered throughput. |
| Layout / tab response | p95 ≤50 ms for tab switch/zoom; no main-thread operation over 100 ms in sustained resize/spawn/close traces. |
| Warm first window / new cmd session | p95 ≤750 ms / ≤500 ms, 20 runs; interactive prompt-ready measured by helper, not a guessed sleep. PowerShell profile cost reported separately. |
| Close completion | p95 ≤1 s for cooperative fixtures, no UI block; timeout UI at 3 s; all owned resources accounted for. |
| Queue/history correctness | No silent control-event or accepted-byte loss; finite queues and configured history under flood/stall. |
| Output correctness | Exact synthetic input byte streams and final-output sentinels; stable terminal fixture state across splits/tabs. |
| Portable release size | Initial target ≤30 MiB compressed, excluding optional fonts and symbols; record actual executable/archive sizes. |

These are engineering targets requiring the baseline in R03 and the final gate in R39. If any cannot be met, attach measurements and profiling evidence, optimize the dominant cost, or explicitly amend the target with a documented product tradeoff. Do not silently relax criteria or claim compliance from architecture alone.

## 7. Roadmap and delivery rules

The issue catalogue in [docs/issues/README.md](docs/issues/README.md) maps stable R-numbers to actual GitHub issues. Each draft includes its goal, scope, acceptance criteria, checks, and hard dependencies. Numeric ordering is topological, not a requirement to serialize independent work.

| Phase / milestone | Tasks | Outcome and gate |
|---|---|---|
| P0 — Reproducible baseline | R01–R04 | Locked Windows crate, CI, reproducible compatibility harness/baseline, documented minimal dependency patch route. |
| P1 — Safe session foundation | R05–R10 | Bounded events/input, explicit teardown, Windows resource/quoting fixes, cancellable asynchronous startup. |
| P2 — Terminal correctness and efficiency | R11–R21 | Capped history, viewport rendering, sizing/identity, keys/copy/paste/mouse/Unicode/IME, protocol and cursor correctness. |
| P3 — Multiplexer MVP | R22–R30 | Separate session/layout state; independent split terminals, resizing/focus/move/zoom, close/exit behavior, persistent live tabs. |
| P4 — Product polish | R31–R35 | Validated settings, remappable commands, refined chrome, privacy-conscious errors, keyboard/high-contrast access. |
| P5 — Windows release gates | R36–R40 | Compatibility matrix, leak/stress evidence, selected renderer, performance report, portable distribution and documentation. |
| P6 — Optional enhancements | R41–R46 | Bounded search, cold layout restore, cross-tab moves, deliberate links, extended keyboard protocol, accessibility path. |

**Recommended first issue: R01**, then R02/R03. R04 provides patch infrastructure, not one giant compatibility rewrite. Regressions and fixes are separate focused issues.

Architectural critical paths:
- R01 → R03 → R04 → R05/R07/R08 → R10 → R23 → R24/R28/R29.
- R04 → R11 → R12/R13 → R18/R19/R21; correctness and history work precede final renderer/performance evaluation.
- R22 pure layout/session contracts can proceed after R01 independently of backend repairs.
- R36/R37/R38 produce evidence for R39; R40 is blocked by the release gate.
- P6 is not required by R40 and cannot block MVP delivery.

Every task is intended as one focused PR. If a task exposes a larger upstream change, split a new prerequisite issue and update the dependency catalogue before coding unrelated work. No feature is complete solely because a button exists; its lifecycle, error, and bounded-resource behavior must pass the listed checks.

## 8. Risk register and review conclusions

| Risk | Severity | Mitigation / release implication |
|---|---|---|
| Published widget is unsuitable unchanged for correctness/lightweight goals | High | Explicit patch budget and isolated adapter; P1/P2 gates before expanding UI. |
| Bounded queue deadlock or lost terminal replies | High | Lock-order audit, coalescing only replaceable state, sticky exit, saturation fixtures; R05/R06. |
| Close/spawn leaks or Windows version-specific blocking | High | Existing backend audit, async completion, output drain, old/new Windows matrix; R07–R10/R37. |
| Session exits close the wrong pane | High | Source example is unsafe to copy; route by stable SessionId and test background exits; R22/R28/R29. |
| Hidden tabs accidentally lose subscriptions or widget identity | High | Registry-based subscriptions and identity-aware diff; R13/R29. |
| Clipboard and keyboard behavior corrupts terminal commands | High | Exact-byte fixture suite, selection_to_string, mode-aware paste, AltGr/IME testing; R14–R19. |
| Monospace rendering clips wide/combining text or hides cursor | High | Local rendering regressions and focused upstream backports; R18/R21. |
| Renderer/features dominate RAM | High | Strip forced defaults, benchmark two profiles, choose from evidence; R04/R38/R39. |
| History line cap does not bound all Unicode auxiliary data | Medium | Geometry/queue caps, pathological text stress, document residual limits; R11/R37. |
| Framework API evolution | Medium | Exact release sources, committed lockfile, patch ledger and deliberate upgrades; R01/R04. |
| Shell-specific Windows quoting differs from CRT rules | High | Separate program/args, PowerShell/cmd fixture coverage and no generic string joining; R09. |
| Canvas lacks adequate screen-reader semantics | High for affected users | Keyboard/high contrast in MVP; honest support statement and R46 accessibility gate. |
| Too many optional features undermine simplicity | Medium | P6 separate; no daemon, terminal persistence, animations, multi-window service, or new PTY in MVP. |

Review performed before issue publication:

- Ownership covers splitting, moving, closing, restart, late spawn completion, hidden tabs, and shutdown of the entire window.
- Last-pane behavior is explicit and does not assume that pane_grid can represent an empty root.
- Subscriptions are driven by live sessions, not the visible tree. Exited sessions retain readable output without idle backend work.
- Shortcut capture precedes terminal handling; clipboard/IME/AltGr have explicit routing and test cases.
- History/snapshot/input/event budgets are distinct; there is no claim that one bounded channel makes every queue bounded.
- Private source APIs are identified as required extensions. Release master fixes are distinguished from published 0.8.0.
- The same PTY/emulator remains throughout. Platform fixes stay in the existing backend rather than introducing a parallel implementation.
- Natural exit output, ConPTY output draining, spawn failures, and final cleanup are release blockers.
- Native window decorations reduce bespoke Windows code; user settings are loaded explicitly rather than watched.
- No measured performance numbers have been invented. Renderer choice, OS support, and release budgets remain testable gates.
- Hard dependencies refer only to earlier tasks; optional work has no reverse dependency into the MVP.
- Current planning verification is documentary. Cargo checks cannot operate on an absent manifest, and Windows runtime profiling must wait for implementation.

## 9. Research provenance

Downloaded published archives and inspected sources have the following SHA-256 values (lowercase normalized). Keep these as provenance, not as a substitute for Cargo.lock checksums:

| Archive | SHA-256 |
|---|---|
| iced 0.14.0 | `000e01026c93ba643f8357a3db3ada0e6555265a377f6f9291c472f6dd701fb3` |
| iced_term 0.8.0 | `e1e287970417db55e0b395a966f061f10c7d3b03fd146d0b8806678d8c063103` |
| iced_widget 0.14.0 | `b698f173f370d959cac589c2000268b44862543d6995cf8f2934cf07afad2e54` |
| iced_runtime 0.14.0 | `d1889b819ce4c06674183242e336c8d49465665441396914dc07cc86f44fa8d4` |
| alacritty_terminal 0.25.1 | `46319972e74179d707445f64aaa2893bbf6a111de3a9af29b7eb382f8b39e282` |

Additional inspected sources: [Iced pane state](https://github.com/iced-rs/iced/blob/0.14.0/widget/src/pane_grid/state.rs), [axis mapping](https://github.com/iced-rs/iced/blob/0.14.0/widget/src/pane_grid/axis.rs), [event subscriptions](https://github.com/iced-rs/iced/blob/0.14.0/futures/src/event.rs), [keyboard subscriptions](https://github.com/iced-rs/iced/blob/0.14.0/futures/src/keyboard.rs), [executor contract](https://github.com/iced-rs/iced/blob/0.14.0/futures/src/executor.rs), [Tokio executor](https://github.com/iced-rs/iced/blob/0.14.0/futures/src/backend/native/tokio.rs), [Alacritty event loop](https://docs.rs/crate/alacritty_terminal/0.25.1/source/src/event_loop.rs), and [Windows backend](https://docs.rs/crate/alacritty_terminal/0.25.1/source/src/tty/windows/).


## 10. Planning completion record

Created and verified **46 GitHub issues** across **7 milestones**: 40 MVP tasks and 6 optional follow-ups. The [complete issue index](docs/issues/README.md) links every issue and prerequisite; [GitHub roadmap issues](https://github.com/shakodev/pike/issues?q=is%3Aissue%20label%3Aroadmap) are live. Recommended first task: [R01 / issue #1 — locked Rust and Windows baseline](https://github.com/shakodev/pike/issues/1).

All 133 dependency links were checked against the actual issue numbers. Bodies, titles, four classification labels per issue, milestone assignments, uniqueness, and phase counts were verified. Every MVP task is included transitively in the release gate; optional tasks are outside it. Local drafts and publication records are preserved under docs/issues/.

The three required Cargo checks were attempted and each exited with code 1 because Cargo.toml does not exist. No runtime behavior or release performance was measured; those remain implementation gates. This planning task added only PLAN.md and the issue catalogue/records, preserved AGENTS.md and .gitignore, and did not implement features, initialize Cargo, vendor dependencies, commit, or push files.
