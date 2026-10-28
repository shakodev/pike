# Combined reader, writer and deadline validation

Exact checked local source `6b5bc81c1bdf4bbb6e4944792ce2a95d78ef56e0`
integrates merged #67/#68 at `1dfeba4992e8c99f18487d621fe99279571d5dcb`.
Its C-owned portable and native read regressions are unchanged. Deadline
preparation always runs before the pending-read idle gate, and writer service
remains at the end of each turn. This is the fourth #20 review candidate;
previous unsuccessful rounds and all observed native failures stay recorded.

`checks.json` records exact commands, selected Cargo JSON build artifacts and
hashes, source/probe hashes, counts and eighteen fixture PIDs verified absent.
Local formatting, denied-warning Clippy for Alacritty/application/adapter,
application/example/doc tests and all 159 Alacritty units, 45 reference cases
and one doctest pass. Both renderer profiles pass protocol, widget geometry,
Unicode and selection-copy release probes. Their bounded events/cleanup/case
counts are retained here, alongside complete adapter failure logs.

The independent, unchanged public-API probes compile against the explicit
rlibs selected by `cargo build -p alacritty_terminal --message-format=json`,
instead of guessing which cached feature variant is newest. After 300ms,
the sync probe records three write attempts, output flushed, queued zero /
outstanding two; the read probe records four attempts and its tail marker
visible. Both acknowledge requested shutdown. Their original scratch-source
warning is retained without changing production lint policy.

These are **macOS** checks. Full adapter suites still report existing bounded
cleanup timeouts: wgpu 90 passed / five failed / one ignored; tiny-skia 89 /
six / one. Prior source-identical controls and prerequisite #67 controls
reproduce this class without changing deadlines. The files do not label those
suites passing or claim Windows behavior. Strict native combined validation,
independent final review and physical-desktop limitations remain explicit in
the PR and [verification](../../../verification/issue-20.md).
