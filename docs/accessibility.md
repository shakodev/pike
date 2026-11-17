# Keyboard access and contrast

Pike provides a keyboard command menu and explicit high-contrast themes. Windows
Narrator behaviour and Windows accessibility semantics have **not been verified**
for this implementation. Do not treat keyboard access or a visible label as a
screen-reader support claim.

## Keyboard controls

Open **Commands and shortcuts** with **F1** by default, or your configured `help`
shortcut. The menu shows the effective bindings, including unbound commands.
It provides keyboard routes for creating, selecting, splitting, moving, resizing,
maximizing, renaming and closing panes/tabs, plus copy, paste, history, font zoom,
profiles and settings reload. Ordinary Tab/Shift+Tab remain terminal-owned while
the menu and dialogs are closed.

| Context | Keys |
| --- | --- |
| Commands | Tab / Shift+Tab or Down / Up selects; Home / End jumps; PageUp / PageDown moves eight rows. Tab wraps. Enter or Space runs the selected available action. Escape returns to the original terminal. |
| Rename | Tab / Shift+Tab cycles Name, Cancel and Save. Enter saves from Name or chooses the selected button; Space activates a selected button. Escape cancels. Text editing uses the platform text-input shortcuts (Cmd+A on macOS, Ctrl+A on Windows). IME composition keeps its existing priority over traversal and submission. |
| All tabs | Tab / Shift+Tab or Down / Up selects; Home / End jumps; Enter activates; Delete requests a targeted tab close; Escape returns without changing tabs. |
| Close confirmation | Cancel is the default. Tab / Shift+Tab or Left / Right changes selection; Enter or Space chooses; Escape cancels. |
| Slow application cleanup | Wait is the default. The same navigation chooses Wait or Force close; Escape waits. Force close retains its existing process-exit semantics. |
| Font zoom | Ctrl+Shift+Plus / Minus changes the active terminal font within 10–28 DIP; Ctrl+Shift+0 restores the configured size. All three actions are also in Commands. |

Command and dialog selections have an outline and a `›` marker. Rename selections
scroll into view, and close-dialog actions stay visible while the explanation
scrolls. The menu and dialogs temporarily suspend terminal typing and restore
the existing terminal owner on dismissal. A failed/exited pane has no running
terminal to focus; its retry/close commands remain reachable through Commands.

Copy uses the terminal's existing selection. This work does not add keyboard
selection or screen-reader navigation of terminal text.

## Error recovery

Recovery actions follow the command list: each retained notice offers Dismiss
and any applicable retry/default-shell action. When diagnostics were explicitly
enabled with `--diagnostics`, Copy diagnostics and Clear diagnostics follow them.
Use End to reach the last row quickly, then move backward as needed.

Actions retain the error and terminal identities from that menu opening. If a
notice is resolved, removed, or no longer eligible for retry, the action is
disabled rather than redirected. Reopen Commands to include newly arrived
notices. Clipboard retry is available only for the terminal from which the menu
was opened; background errors cannot paste into the current terminal. Window
focus loss retires old menu actions. Diagnostics remain opt-in, bounded and
metadata-only as described in [errors.md](errors.md).

## High-contrast themes

Set either value at the top level of `settings.toml`, then explicitly reload
settings (Ctrl+Shift+F5 by default):

```toml
version = 1
theme = "high-contrast-dark"
# Alternative: theme = "high-contrast-light"
```

These themes use black/white chrome text, explicit outlines, and a yellow or
dark-blue selection accent. Default terminal foreground/background and cursor
follow the selected theme. Terminal ANSI colours, OSC overrides and application
colours remain authoritative; arbitrary TUI output is not guaranteed to meet
contrast targets. Font zoom affects terminal cells, while native monitor scaling
continues to determine the application's logical-pixel scale.

These are explicit settings. `theme = "system"` follows light/dark changes; it
does not detect Windows Contrast themes or import the user's system contrast
colours. No watcher or polling loop is added.

## Screen-reader support and Windows follow-up

Inspection of the pinned Iced widget/core, window adapter and terminal widget
sources found no application accessibility tree or Windows UI Automation bridge
for Pike's canvas terminal. Visible control labels, tooltips and keyboard actions
are present, but their actual Narrator exposure remains unassessed. There is no
continuous transcript, duplicated terminal buffer, or background accessibility
worker. macOS rendering and synthetic input cannot establish Windows support.

Keep [issue #35](https://github.com/shakodev/pike/issues/35) open until a supported
Windows machine has recorded:

1. A physical keyboard-only create/split/move/resize/rename/overflow/copy/paste/
   recovery/close journey, including focus after cancellation and window blur.
2. Both explicit contrast themes and Windows Contrast-theme interaction at
   supported font sizes, fractional DPI and real monitor transitions.
3. Narrator observations for every dialog/control, accessible names, selected
   states, focus changes and the distinction between chrome and terminal text.

[Issue #46](https://github.com/shakodev/pike/issues/46) owns investigation and a
measured implementation decision for a terminal accessibility bridge. Carry the
actual Narrator gaps from this assessment into that work; do not promise full
screen-reader support from this macOS implementation.
