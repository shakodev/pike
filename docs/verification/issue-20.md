# Issue #20 — terminal protocol and metadata policies

Review-corrected native Windows profiles passed at source `187f6e6`;
exact run links, source hashes and retained evidence are in the final section.

The issue is the self-contained plan; PLAN.md is absent. Work started from
merged #5/#11/#12 main and incorporated merged #13 (`0ae689c`). No dependency,
feature, lockfile, additional PTY or workspace UI integration is added.

## Event contract

Every variant in pinned Alacritty 0.25.1 `event::Event` has an explicit policy.
`Terminal::subscription` retains the originating terminal ID in every
`Event::BackendCall`; callers route it to that session and interpret its Action
in the same ID context. Workspace SessionId/metadata integration remains #23.

| Event | Policy |
| --- | --- |
| ChildExit(code) | Retain actual status and return ChildExit, including while hidden |
| Exit | Return Shutdown { exit_code }; absent status remains None, never invented success; backend completion remains separate |
| PtyWrite | Admit the entire reply through the existing ordered input path without scrolling to bottom |
| ColorRequest | Resolve indexed 0–255, foreground, background and cursor using the current configured palette and parser-ordered OSC override; cursor falls back to foreground |
| TextAreaSizeRequest | Format aggregate virtual-grid pixels, rounding rows/columns times fractional logical cell metrics once; native integer cell metadata and physical DPI are separate |
| Title | Sanitize controls/bidi overrides and bound to 4,096 UTF-8 bytes before model, bridge and application retention; return ChangeTitle |
| ResetTitle | Return ResetTitle; VT reset now emits it as well as resetting the model |
| Bell | Return Bell; bursts coalesce to one pending notification; no sound or host notification is installed |
| MouseCursorDirty | Return MouseCursorDirty and update cached mode metadata without copying cells |
| CursorBlinkingChange | Return CursorBlinkingChanged with the current blink flag, updating mode metadata without copying cells; renderer remains solid with no blink timer |
| Wakeup | Preserve visible-output refresh and hidden-session snapshot suppression |
| ClipboardStore / ClipboardLoad | OSC52 disabled in parser configuration, including after history changes; bridge drops injected requests without retaining payload or consuming FIFO capacity; backend never invokes clipboard formatters |

Invalid/non-query color indices are unsupported and do not invoke a formatter.
User-initiated keyboard copy/paste retains its separate existing policy; OSC52
never reads/writes a system clipboard. Empty titles remain empty; ResetTitle
requests the consumer's default label. Latest-title and bell slots intentionally
coalesce, while controls retain FIFO ordering and reserved lifecycle capacity.

Alacritty's saved-title stack holds at most 64 bounded titles (256 KiB maximum
title payload at pathological saturation); keyboard-mode limits are unchanged.
Sanitizers discard the original String capacity. Title set/push/pop traces no
longer expose titles. Alacritty Event Debug redacts title, reply and clipboard
payloads; this patch logs no terminal transcript or clipboard content.

One theme object serves query and rendering paths so host theme changes cannot
leave a separate response palette stale. Deferred color formatters capture the
model override at the query's parser position: set/query/reset still reports
the queried override when the entire batch was parsed before UI consumption.

Replies/focus reports use the existing whole-input admission and packed ordered
writer. Rejection returns redacted InputRejected and requests only that session's
close; required replies are not silently lost while claiming a healthy session.
Closing cancels outstanding bytes under the existing shutdown contract rather
than promising a graceful flush. Post-close queries are rejected.

## Focus ownership

The actual window owner creates one `WindowFocus`, shares it with all of that
window's terminals (including hidden/new sessions), and records real Focused/
Unfocused events. Default/unknown focus is unfocused. Bind it with
`Terminal::set_window_focus` before showing a terminal. The current single-window
application retains this authority during asynchronous startup and binds it on
attachment; no workspace UI is added.

`Terminal::set_focused(bool)` supplies pane focus, gated by the shared window
state and visibility. Backend state deduplicates transitions and sends CSI I/O
only with DEC private mode 1004 enabled. Enabling the mode is not a transition.
Hiding a focused session emits its one focus-out report before releasing the
viewport; set_visible returns any scoped admission rejection.

The widget updates that same window authority from real window events. Neither
terminal-ID diff nor new tree creation invents window focus. Mouse/operation
focus stays transient per widget, and report dedup stays session-owned. No
polling or independent report queue is installed.

CSI 14t uses virtual logical-grid pixels, matching the existing cell units:
80 columns with 8.5-pixel cells report width 680. `TextAreaSize` carries aggregate
u32 dimensions independently of native `WindowSize`; products are computed in
f64, rounded once and bounded to 1..u32::MAX. PTY resize/count dedup is unchanged.
Monitor rescaling does not multiply these virtual units or reshape fonts.

## Verification and reproduction

Focused tests replay VT through the pinned parser and backend, covering exact
sizes/colors/order, a deferred set/query/reset batch, RIS title reset, focus
mode/transition gating, hidden metadata/status, disabled OSC52 after settings
changes, invalid indices and scoped reply rejection. Bridge regressions verify
strict retained bounds under 10,000-event clipboard/title/bell floods plus
ordered control/lifecycle delivery. Alacritty tests verify sanitized title
payload/capacity/stack limits and redacted Event diagnostics.

The opt-in `terminal_protocol` release probe launches two copies of itself as
controlled real PTY children. It asserts exact ordered bytes for different
palettes and stable IDs, sanitized title/reset/bell handshakes, 10,000 metadata
changes interleaved with ten queries per session, disabled/duplicate/hidden
focus behavior, actual exit codes 41/42 and drained natural completion. Session
20002 starts hidden and receives its own queries and metadata. Probe output
contains synthetic bytes and numeric metadata only. Cleanup is awaited after
preparation/verification failure; completed.txt is written only after success.

The byte oracle covers every reply end to end. Query-route counters are scoped
to the fixture phase and require exactly 17 palette, one pixel-size and one
character-size request per child on BOTH platforms. Native Windows source
`31aa375` passed these counts, focus and drained exits; the initial fixture
failure counted out-of-phase console negotiation. No query-byte/order/origin
oracle was relaxed. Review corrections described below require fresh native runs.

Use a worktree-local CARGO_TARGET_DIR and CARGO_BUILD_JOBS=4:

```text
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --locked --examples
cargo clippy --locked -p iced_term --all-targets --no-default-features --features iced/wgpu,iced/tokio -- -D warnings
cargo test --locked -p iced_term --lib --no-default-features --features iced/wgpu,iced/tokio
cargo clippy --locked -p alacritty_terminal --all-targets -- -D warnings
cargo test --locked -p alacritty_terminal
cargo build --release --locked --example terminal_protocol
```

On Windows stage the pinned runtime into the actual target directory with
`tools/prepare-conpty.ps1 -TargetDirectory <target>`, then run
`<target>/release/examples/terminal_protocol.exe <fresh-output>`.
Repeat application checks/builds with `--no-default-features --features renderer-tiny-skia`
and adapter checks with `--no-default-features --features iced/tiny-skia,iced/tokio`.
The Unix fixture uses system stty raw -echo for its inherited PTY input only.

Local wgpu formatting, denied-warning app/adapter/Alacritty Clippy, application/
example tests, 83 adapter tests and the Alacritty suite (149 unit, 45 reference,
one doctest) pass on macOS. The real-PTY release probe passes both identities,
all 38 queries, 20,000 metadata changes, focus reports and drained exits. Its
first run reproduced the missing RIS ResetTitle, now fixed with a regression.

The isolated [Windows workflow](../../.github/workflows/terminal-protocol.yml)
runs the adapter suite and release probe for both renderers, retaining source,
compiler/runtime/executable hashes, checking both fixture PIDs disappear and
recording externally sampled short-probe CPU/private/working-set metadata.
The final section records native evidence and exact run links. These headless
observations do not establish GUI idle budgets, input/render latency or physical
Windows desktop/DPI behavior; none of those is claimed.

## Initial independent review corrections

Review reproduced a false CSI-I after Window::Unfocused on A, replacing the ID
with B, programmatic focus and redraw without a real Window::Focused. The shared
owner-bound WindowFocus fixes replacement and completely new trees. The public
real-renderer widget probe now enables mode 1004 in its controlled children and
checks admitted bytes: no report for unknown/unfocused replacement/new trees,
exactly one CSI-I for real focus/normal focused creation, no repeated redraw
report, and one CSI-O for actual blur. Its children consume these synthetic
reports until requested cleanup.

The other reproduced bug was per-cell rounding: 80 times rounded(8.5) reported
720 instead of 680. Aggregate TextAreaSize removes that conversion from protocol
formatting. A direct VT regression checks 680x320 and another fractional grid;
the two-session release probe now uses 8.5-pixel cells in session 20002 and expects
exactly 80x20 characters / 680x320 virtual pixels. The widget event probe verifies
that Rescaled(1.5) leaves protocol pixels unchanged, with no font/grid/native
resize work. These fixes do not claim physical desktop/DPI verification.

## Native Windows review evidence — 2026-10-27

All eight jobs passed at exact source
`187f6e68ecdcec0e6bb6473308fe9125147e1f56`:

- [Protocol probes, both renderers](https://github.com/shakodev/pike/actions/runs/40496875553): 85 adapter tests per profile (one existing profile test ignored), exact 17+1+1 query routing and complete reply bytes/order for each ID, title/reset/bell, 20,000 total metadata changes, focus gating, statuses 41/42 and drained natural cleanup.
- [Public widget probes, both renderers](https://github.com/shakodev/pike/actions/runs/40496875706): the exact unfocused replacement/new-tree regression, normal focused creation, no duplicate redraw report, one real blur report, virtual query pixels unchanged at Rescaled(1.5), plus preserved 125/150/200% translation/clipping and modal coverage.
- [Full Windows CI, both renderers](https://github.com/shakodev/pike/actions/runs/40496875528): formatting, denied-warning app/vendor Clippy, application/example/doc tests, explicit vendor suites, release builds/package and native launch/input/shutdown probes.
- [ConPTY audit, Windows 2022/2025](https://github.com/shakodev/pike/actions/runs/40496875642): both cohorts passed the existing native ownership/resource/lifecycle suite.

The host is native x64 MSVC, Rust 1.97.1, Windows Server 2025 build 26100 with
pinned ConPTY 1.25.260930003. [Retained evidence](../baselines/issue-20/windows/)
contains compiler/runtime/source/executable hashes, adapter results, exact reply
bytes encoded losslessly as hex, event/cleanup records and source provenance.
Text records are normalized to LF with original artifact hashes preserved in
provenance.json. Both protocol child PIDs and all three widget child PIDs were
absent after acknowledged cleanup in each renderer run (ten fixtures total).

The native second session uses 80 columns with fractional 8.5-pixel cells and
verifies CSI 14t returns 680x320, while CSI 18t returns 80x20. The source-level
fractional regression and public non-1x rescale check pass on both platforms.
Locally, both profiles passed required formatting, denied-warning application/
adapter/Alacritty Clippy, application/example tests, 83 adapter tests, and both
release probes; the explicit Alacritty suite passed 149 unit, 45 reference and
one documentation test. All ten local review-fixture PIDs also exited.

The opt-in collector sampled the short release protocol parent every 10 ms;
these observations include parent initialization, protocol/flood handling and
shutdown and exclude compilation, synthetic child resources and all GUI work:

| Profile | Wall seconds | Last sampled process CPU seconds | Samples | Maximum sampled private MiB | Maximum sampled working set MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| wgpu | 0.118624 | 0.1250 | 2 | 13.18 | 10.42 |
| tiny-skia | 0.111539 | 0.0625 | 5 | 13.17 | 10.90 |

These sparse samples are not lifetime peaks, idle budgets, responsiveness
percentiles or a causal renderer comparison. Last sampled process CPU can
span multiple threads; no one-core utilization claim is inferred. Source
bounds are tested directly rather than inferred from these brief measurements.
Real physical monitor transitions and interactive Windows desktop behavior
remain unverified release-matrix work, with no claim of physical DPI testing.

## Integration with merged Unicode rendering

After the review evidence above, main merged #18 at
`487f5bca537d9810b971b5bd04b2b6d3edd9bf26`. Local integration source
`bab757360db9eab72f815bc71c18a2c586884cf8` retains its shaped Unicode buffers,
wide cursor, cache generations and explicit cell/ancestor clips. Conflicts were
resolved by retaining the merged drawing implementation and changing only its
palette accesses to the approved single Backend theme owner; R20 focus routing
remains outside drawing. No glyph helper or snapshot/cell interface was changed.

Formatting, denied-warning app/adapter Clippy, application/example tests and
90 explicit adapter tests (including the seven merged Unicode regressions) pass
for both local renderer profiles. Protocol, public-widget geometry/focus and
Unicode two-font/four-scale release probes are repeated on the combined source.
The 187f6e6 native evidence above is preserved with its original exact source;
combined-head native reruns are pending until the integration is pushed.

## Combined-head transport investigation

Combined source `938a48b` passed all local checks and six release probes. Native
full CI, Unicode, widget geometry and resource audits passed, as did the
protocol tiny-skia job. Its [wgpu protocol job](https://github.com/shakodev/pike/actions/runs/40498774695)
timed out after session 20001 title/reset/bell but before its initial query read
completed; session 20002 completed all strict 17+1+1 routing, focus and drained
status-42 checks. Both backends acknowledged cleanup. The
[failure evidence](../baselines/issue-20/windows-integration-failure/) preserves
exact source/hash/event/cleanup records; no root cause is claimed yet.

The diagnostic fixture now retains only bounded synthetic test bytes: at most
256 formatted-reply records of 128 bytes each, per-child partial-input traces
capped at 4,096 bytes and a latest-stage marker. Runtime/vendor code and strict
response/order/origin oracles are unchanged. The focused Windows job exercises
ten independent launches per renderer and stops on first failure, without
raising the existing deadline or retrying until green. This investigation
remains a native verification blocker until its cause/coverage is resolved.

Diagnostic source `3e5ba2f` passed ten independent launches per renderer
([native run](https://github.com/shakodev/pike/actions/runs/40500354892)); all
other native workflows also passed. These green diagnostic runs do not repair
or explain the original timeout. Their traces show an extra startup DA reply
in phase zero for each session, explaining the earlier phase-counter fix only.

The next diagnostic revision buffers reply/read traces during the exchange,
then writes them after completion to reduce observer I/O. It snapshots actual
Windows input modes at startup, before/after RIS, before initial queries and
later phase boundaries (at most eight snapshots per child). On a failed verdict
only, the parent records each origin's input-writer counters and sends 512
explicitly marked question marks through the same input path to release a
blocked synthetic read and persist its partial trace. A one-second diagnostic
grace follows the already-failed verdict; the original acceptance deadline and
strict response checks are unchanged, and this cannot create a pass. A child
that cannot consume the diagnostic stimulus may have no retained read trace.
All diagnostic bytes are controlled fixture data; no production logging or
runtime/vendor source changes are involved.

Buffered source `7fb8683e1d9af4dac1c931c4056c34ff8b53cb59` also passed
[all twenty independent native launches](https://github.com/shakodev/pike/actions/runs/40502419892),
with 92 adapter tests per profile (one existing ignored). Every one of its
forty child traces retained input mode 512 (`ENABLE_VIRTUAL_TERMINAL_INPUT`)
at startup, before/after RIS and every later phase boundary. This is evidence
for these passing trials, not proof about the earlier failed run. Its
[widget](https://github.com/shakodev/pike/actions/runs/40502420018),
[Unicode](https://github.com/shakodev/pike/actions/runs/40502419874),
[full CI](https://github.com/shakodev/pike/actions/runs/40502419867) and
[two-OS audit](https://github.com/shakodev/pike/actions/runs/40502419841)
checks also passed.

[Both diagnostic waves](../baselines/issue-20/windows-diagnostics/) retain all
40 attempted launches / 80 exact child streams, per-origin formatted replies,
events/cleanup, source/runtime/executable provenance, resource observations
and hashes of every downloaded artifact. A local deliberately stopped child
verified that the failure-only read-release path still exits unsuccessfully,
omits the success marker and acknowledges cleanup; that negative experiment
is labeled macOS and makes no native Windows claim. No failing byte trace or
production repair has been established. The original native timeout remains
an explicit verification blocker pending independent review; the PR stays draft.

The automatic native run on documentation-only source `5997177` then
[reproduced the failure with buffered diagnostics](https://github.com/shakodev/pike/actions/runs/40503069279):
wgpu stopped at trial 01; all ten tiny-skia launches passed. The
[complete attempted-launch record](../baselines/issue-20/windows-buffered-failure/)
preserves all eleven launches and original artifact hashes. This time session
20002 stalled, despite formatting all nine initial responses. Before failed-
verdict diagnostic padding, it still had 148 bytes in the bounded input FIFO,
55 acknowledged OS-written bytes, and open admission. Session 20001 fully
passed and completed status 41/drained. The failed origin acknowledged requested
cleanup without natural exit/output drain; no child trace was persisted, so
its actual consumption and console mode remain unknown. This is actionable
evidence of queued writer nonprogress; the exact interleaving and any remedy
still require a deterministic regression and independent review.

## Queued-writer correction

The captured 148-byte FIFO remainder led to a deterministic portable regression:
with writable interest already enabled, consume an actual `Poller::notify()`
and its empty readiness batch, then finish the original event-loop turn with
148 admitted bytes and a ready nonblocking sink. The original block leaves
all 148 bytes queued. The [red patch/log/provenance](../baselines/issue-20/windows-buffered-failure/)
reconstruct this without relying on thread timing. Pinned polling documents
that notifications return no readiness events; pinned piper removes its writer
waker when not full. A queue admission during an already-writable turn could
therefore leave no writable event to service its bytes.

The correction attempts pending nonblocking writes on the existing event-loop
wake before updating interest, including a retained partial write when deciding
whether a wake is idle. Capacity exhaustion preserves bytes and waits for the
existing readiness signal. Idle/closed/EOF sessions do no writes. The original
bounded queue, atomic acceptance, error/cancellation and disposing-worker paths
remain in use; no thread, timer, polling loop or dependency is added.

Three deterministic regressions cover the captured phase state, full-capacity
FIFO order across partial/zero/WouldBlock writes, idle/closed/EOF no-write and
notify-only retained-partial progress followed by a propagated write error with
exact cancellation and disposal-before-completion. This corrects a demonstrated
progress defect; the old `938a48b` trace still does not reveal its actual bytes
or interleaving. Native validation of the concrete candidate and independent
review are required before declaring issue acceptance satisfied.

During local candidate checks, the wgpu adapter suite reported five macOS
font/work-test cleanup timeouts (85 passed, five failed, one ignored). The
focused `font_changes_measure_once_and_paint_only_changes_do_not_snapshot`
test failed at the same five-second completion wait with both the candidate
and exact unmodified `5997177` event-loop source, using the identical command.
The original candidate file was restored in `finally`. This establishes an
existing local cleanup limitation; no timeout or Unix lifecycle code is changed
by the writer correction. The complete check result is reported separately
from the deterministic writer regression and native Windows evidence.

Local final candidate checks pass formatting, denied-warning application/
adapter/Alacritty Clippy, application/example/doc tests, all 152 Alacritty unit,
45 reference and one documentation test, and protocol/widget/Unicode release
probes for both renderers. All fourteen owned synthetic fixture PIDs were
absent after acknowledged cleanup. The tiny-skia adapter run reports 87 passed,
three cleanup timeouts and one ignored, alongside the wgpu result above.
The focused original-source control establishes that particular existing
cleanup failure; no claim is made that every failing scheduling interleaving
was independently reproduced. [Candidate checks and control logs](../baselines/issue-20/windows-buffered-failure/)
retain exact counts, restored-source hashes, fixture events/cleanup and PIDs.

## Final integration with approved selection copy

Merged exact main `103e29552148aeb2273996e0d178fa781f6ec680` (PR #66) into
the local writer candidate without conflicts. The copy sink, captured keyboard
binding/modifiers and stable-ID menu checks compose with the existing shared
window focus and single backend palette owner. Writer code/tests are byte-
identical to local candidate `e5f236e`; the strict protocol fixture is unchanged
from `5997177`. Only its workflow provenance list additionally hashes the
changed writer source and regression tests.

Combined local source `6f3e07f` passes formatting, both application/adapter
denied-warning Clippy profiles, application/example/doc tests and all eight
protocol/widget/Unicode/selection-copy release probes. All eighteen synthetic
fixture PIDs are absent after acknowledged cleanup. Alacritty source/tests are
unchanged from the passed 152-unit/45-reference/one-doc suite. The combined
adapter runs report wgpu 92 passed/three cleanup timeouts/one ignored and
tiny-skia 89 passed/six cleanup timeouts/one ignored; these local failures remain
explicitly reported alongside the original-source cleanup control. The
[combined records](../baselines/issue-20/windows-buffered-failure/local-combined-checks.json)
retain source hashes, counts and controlled process identities. This concrete
candidate requires its own native Windows validation and independent review.

## Third review: independent sync deadline and flood-read blockers

Native writer candidate `d405c6d` passed full Windows CI, widget geometry,
Unicode, selection-copy and ConPTY audits, but
[protocol run 40507568597](https://github.com/shakodev/pike/actions/runs/40507568597)
failed wgpu trial 06 after five passes; all ten tiny-skia launches passed.
Both failing origins received the exact initial nine responses and then stalled
in the metadata flood. Writer queues/outstanding counts were zero, with 347 /
323 bytes OS-written; only six / five flood queries reached formatting.
Both raw child streams and mode-512 snapshots are retained in
[all sixteen attempted launches](../baselines/issue-20/windows-writer-candidate-failure/).
This is distinct from the earlier queued-writer failure. The fixture emits no
BSU, so this native failure is not attributed to the sync deadline below.
Implementer C owns the separate reader-continuation prerequisite investigation;
R20 does not introduce a concurrent reader patch.

Reviewer 2 independently reproduced a sync deadline spin at d405 through the
public EventLoop API: a partial writer followed by WouldBlock made 10,392
attempts after 300ms and left synchronized output buffered. The correction now
expires the existing deadline before the idle decision, independently of input
and readiness. Future deadlines do not flush early or emit idle wakeups. The
same independent probe records three attempts, output flushed and requested
cleanup on corrected local source. Its exact source/runner/link provenance is
retained and labeled macOS; no native Windows claim is inferred.

The deterministic matrix covers future/expired deadlines with idle, queued and
retained-partial input, notification-only wakeups and actual socket readiness.
All ten lifecycle tests and the complete 154-unit / 45-reference / one-doc
Alacritty suite pass, alongside formatting, both application/adapter Clippy
profiles and application/example/doc tests. Earlier macOS adapter cleanup
failures remain reported. No timer, polling loop, self-notification, new thread
or reader-continuation production change is added. Pushes remain held until
the separate prerequisite and independent review permit final validation.

## Fourth candidate: merged reader prerequisite

PR #68 merged as exact main `1dfeba4992e8c99f18487d621fe99279571d5dcb`,
closing #67. It is integrated into local source `6b5bc81`. The sole event-loop
conflict is resolved by always calling `prepare_io_turn`, then skipping an
idle turn only when both its result and pending read continuation are false.
Thus productive More turns still expire sync deadlines. The approved bounded
read continuation, duplicate-read suppression, channel/child/resize fairness,
EOF/error handling and `finish_io_turn` writer path are preserved. Both of C's
portable/native regression files are byte-identical to the merged base.

[Final local records](../baselines/issue-20/final-issue-67-integration/)
retain exact selected Cargo rlib paths/hashes and commands. Formatting,
denied-warning Alacritty/application/adapter Clippy, application/example/doc
tests and all 159 Alacritty unit / 45 reference / one doc tests pass. Both
renderer profiles pass all eight protocol/widget/Unicode/copy release probes;
all eighteen owned fixture PIDs are absent after acknowledged cleanup.
The unchanged independent public probes record three writer attempts with
sync output flushed, and four reader attempts with the buffered tail visible,
after 300ms; both acknowledge requested cleanup. These remain macOS contract
checks, not Windows execution claims.

The final macOS adapter runs explicitly report wgpu 90 passes/five cleanup
timeouts/one ignored and tiny-skia 89 passes/six cleanup timeouts/one ignored.
Complete logs preserve the existing bounded cleanup limitation and earlier
unchanged-source controls; no deadlines/assertions are weakened. This is the
fourth candidate for independent review and strict combined Windows acceptance,
with all three prior unsuccessful review rounds retained. No historical native
interleaving or physical Windows desktop/idle/latency claim is inferred.
