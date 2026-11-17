# AGENTS.md

## Goal

Build a polished, Windows-first, **ultra-lightweight terminal multiplexer** inspired by Orca. Prioritize fast, reliable split terminals and minimal idle RAM/CPU over extra features.

## Stack

- Stable Rust; **Iced 0.14** with `pane_grid`; **iced_term 0.8** (Alacritty terminal emulation / Windows ConPTY).
- Prefer existing, version-compatible APIs; inspect installed crate sources and examples before coding. Pin dependencies in `Cargo.lock`.
- No Electron, Tauri, embedded browser, or second PTY implementation without a demonstrated need.

## Design rules

- Separate pane/layout state from terminal session/process state; use stable session IDs.
- Every pane has an independent terminal session. Splitting, moving, and resizing must not unexpectedly terminate a session.
- Use event-driven updates; avoid busy polling, needless redraws, unbounded buffers/scrollback, and unnecessary background tasks.
- Keep modules small, dependencies minimal, and behavior testable. Avoid `unsafe` unless justified and documented.
- Handle session shutdown, resize, copy/paste, errors, and resource cleanup explicitly. Never log terminal contents or credentials by default.

## Workflow and verification

- Work in small, verifiable increments; do not implement unrelated features.
- Inspect the current branch, working tree and relevant changes on main early. Resolve integration conflicts before final verification, rather than repeatedly validating an outdated branch.
- **Use targeted local checks during development.** Select only the formatting, lint, test and native fixture checks relevant to the changed behavior and affected targets/packages. Run patched-dependency tests explicitly when the relevant vendor code changes.
- Do not automatically run full-workspace or both-renderer checklists for every edit. A full suite is appropriate only when the change actually affects that scope; explain why it is needed. Documentation-only changes do not require Rust builds or tests.
- Diagnose and fix failures locally with the smallest reproducer. Do not use GitHub Actions as an iterative debugging loop. Do not repeat successful checks unless a relevant subsequent change invalidates their results.
- **GitHub Actions is disabled for this repository by owner request. Perform verification locally instead.** Do not re-enable Actions, dispatch workflows or create replacement remote CI without an explicit user request. Preserve workflow files so they can be reused if the owner later chooses to re-enable Actions.
- **If the owner explicitly enables CI for a task, run at most one final CI round**, after implementation, integration and targeted local validation are complete. Do not repeatedly dispatch full CI or rerun failed jobs. Investigate any final CI failure locally and report remaining limitations; another remote run requires an explicit user request.
- Validate affected Windows behavior and measure release memory, CPU and responsiveness when relevant to the change. Report actual measurements rather than guesses. Distinguish synthetic checks from physical keyboard/monitor verification; do not claim unperformed manual checks passed.
- In the final report, state that verification ran locally, list the relevant commands/results, summarize changed files and disclose remaining limitations. If the authorized final CI round was used, link it and report its result separately.
- These verification rules replace blanket checklists in older project documents or skills. Keep checks proportional to the actual change.

## When GitHub CI is useful

CI provides an independent clean-machine check, coverage on additional operating-system versions, shared pull-request results and retained build/test artifacts. It is useful for collaboration and release gates, but remote builds, queues and cache misses add latency. For this repository's normal development, targeted checks on the local Windows machine are the preferred feedback loop; remote CI is optional and owner-controlled.
