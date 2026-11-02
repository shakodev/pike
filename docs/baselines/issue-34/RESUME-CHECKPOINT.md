# R34 resumed lower-host checkpoint

Owner resume explicitly revoked the prior stop. Original source/evidence and all
refs/caches remain preserved on `issue-34-recoverable-errors`; no reset/rewrite,
self-merge, external message or Actions run occurred. Full issue34 remains active;
this is an early review checkpoint, not an acceptance/completion claim.

New source: fallible native resize propagation in Alacritty, one bounded current
receipt/checked serial in iced_term, host retry/resolution, safe link-open action,
and a fixed user32 cleanup prompt on the existing executor and Wait/Force policy.
The native dialog uses DialogBoxIndirectParamW and standard controls, requiring
no common-controls activation. Its aligned template and callback context remain
live through modal return; synchronized cancellation/publication/WM_NCDESTROY
prevents stale HWND use. Wait is default, Escape/X wait, explicit Force alone
requests process exit, and current timeout tokens reject stale/duplicate replies.

Windows API references inspected: [DialogBoxIndirectParamW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-dialogboxindirectparamw),
[WM_INITDIALOG](https://learn.microsoft.com/en-us/windows/win32/dlgbox/wm-initdialog),
[EndDialog](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enddialog),
and [PostMessageW](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-postmessagew).
Installed pinned sources inspected again: iced 0.14.0, iced_runtime 0.14.0,
iced_term 0.8.0 / Alacritty 0.25.1 vendored sources and windows-sys 0.59.0.

All checks local, locked, CARGO_BUILD_JOBS=4 and this worktree's own reused target.
First vendor resize selection: 7 passed (`resume-resize-vendor-tests.log`).
Real native PTY retry/serial tests: 2 passed (`resume-native-resize-final.log`).
Original error/privacy/healthy-session host tests: 5 passed (`resume-host-errors.log`).
Current resize host/survivor test: 1 passed (`resume-current-resize-host-green.log`).
Delayed native response and real pool disposal: 1 passed (`resume-delayed-cleanup-host.log`).
Prompt precancel/poison tests: 2 passed (`resume-native-cleanup-model.log`).
Windows lib/bin cross-check passed (`resume-native-windows-check.log`). It is
cross-compilation, not native Windows or physical keyboard/monitor validation.

Preserved red evidence: the first host resize test used a hidden-tab source,
correctly refused by existing visibility guards; corrected to two visible split
panes. An existing local-socket Alacritty fixture was sandbox-denied; the exact
unsandboxed targeted rerun passed (`resume-alacritty-socket-green.log`). Expected
unused presentation warnings remain until the actual33 insertion gate clears.

Remaining in this same full Task: actual32 clipboard lease/config/refusal hooks,
actual33 visual integration and passive space-reserving banners, final Windows
conversion/callback fixtures, final proportional checks, exclusive R1 review,
fixes and one complete Closes #34 PR. No resource or physical-input claim is made.
