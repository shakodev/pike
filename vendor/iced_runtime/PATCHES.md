# iced_runtime patch ledger

## Provenance

- Published crate **iced_runtime 0.14.0**, archive SHA-256
  `d1889b819ce4c06674183242e336c8d49465665441396914dc07cc86f44fa8d4`.
- All published archive files are retained except the delta below. Registry
  installation marker `.cargo-ok` is not an archive file and is omitted.
- Exact upstream revision `.cargo_vcs_info.json`:
  `3997291f318a8bc06fa522f5579836fb3feb94df`, path `runtime`.
- MIT LICENSE is fetched unchanged from that exact upstream revision because
  the published subcrate archive omits the repository-level file.
- Root lockfile selects the same version by path only. Packaged manifests and
  lockfile remain provenance. The root excludes this package from workspace
  roots and runs its affected tests explicitly.

## R34 / issue #34 — acknowledge clipboard writes without reading them back

- Add metadata-only `WriteError::{Unavailable,WriteFailed}` and additive
  `Action::WriteWithResult`, plus `write_with_result` using the existing oneshot
  task primitive. Retain old Write behavior/API.
- The existing iced_winit native writer completes the result. No additional
  backend, thread, readback, polling, retained copied value or iced_core trait
  change is introduced. A canceled result channel simply drops the receipt.
- Explicit Debug for all clipboard actions reports kind and byte count only;
  even an external host formatting a runtime clipboard action cannot leak text.
- Tests drive the actual task stream, native action and one-shot result for
  success, unavailable and failure, plus cancellation and Debug redaction.
- Remove when the pinned upstream runtime supplies equivalent metadata-only
  write acknowledgements and redacted clipboard-action Debug.

## R34 / issue #34 — bounded renderer readiness and failure metadata

- Add typed `RendererEvent::{Ready,Failed(RendererFailure)}` and one registration
  System action. The task uses a capacity-two existing futures channel so Ready
  and one failure remain ordered even before either is consumed. No timer or
  extra runtime/worker is introduced; the pinned Task stream's initial yield is
  retained.
- Pike's new boot entry points defer preparation/configuration until Ready,
  retaining the same App, WindowFocus and input authority. Existing immediate
  constructors remain compatible for already initialized hosts.
- The existing winit host owns one sink and delivers at most one failure after
  its finite compositor recovery budget. Application cleanup continues in the
  existing loop, then its normal Exit returns a fixed graphics Result.
- Tests cover both queued events and pinned stream scheduling; native evidence
  includes first-frame, in-flight preparation, attached children and settings
  cancellation. See docs/baselines/issue-34 for the owner-stop partial checkpoint.
- Remove when upstream exposes equivalent bounded readiness/failure delivery
  through its existing host and task APIs.
