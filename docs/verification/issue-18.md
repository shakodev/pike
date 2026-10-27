# Issue #18 — wide and combining Unicode cells

Based on main `0ae689c` (merged #11/#13), independently of parked #14/PR63.
The authoritative AGENTS.md, self-contained issue, R13 verification and exact
Alacritty 0.25.1, Iced 0.14/iced_term 0.8 sources were read. The rendering part
of upstream iced_term PR73 at `230945e443c7bded8eac0786240210a51a0e8303`
was inspected: moving wide glyph centers alone does not render cell combining
extras or provide a cell clip. No IME or keyboard changes are included here.

## Cell contract

Rendering uses Alacritty's `WIDE_CHAR`, `WIDE_CHAR_SPACER`,
`LEADING_WIDE_CHAR_SPACER` and `Cell::zerowidth` data. A lead owns one or two
terminal columns; continuation and pre-wrap padding cells emit no duplicate
glyph. Combining marks are shaped with their base in one paragraph, including
font fallback, without using a shaped advance to place the next cell.
Unicode glyphs are clipped to their own span and row. Concealed cells emit no
text. Backgrounds precede glyphs and the cursor; a wide cursor covers the lead
and its continuation, including when the emulator's raw cursor points at the
continuation. ASCII keeps its existing pane geometry cache and positions.

Pinned tiny-skia's canvas text path ignores per-text clipping. Unicode therefore
uses cached shaped buffers through Iced’s core raw-text renderer inside
per-span layers. The pinned tiny-skia paragraph path also skips clip masks when
logical advances fit, overlooking glyph ink overhang. Raw buffers retain logical
extents slightly larger than the clip so the actual mask applies even for tall
mark stacks. Their measured text bounds are centered in the terminal span.
The cache belongs to the reconciled widget state and contains only the current
viewport's Unicode spans. Snapshot/cache-invalidation generations rebuild it;
ordinary redraws reuse the shaped buffers. Repeated identical clusters and font
styles within one viewport share their buffer. No global glyph-history cache is
introduced. Iced’s raw primitive conservatively marks its damage as changed on
an existing redraw; it schedules no redraw or background work on its own. Session-ID replacement drops the old
cache. R13 pane-local translation, ancestor clipping, font measurements, PTY
resize deduplication and session ownership stay intact. No task, timer, polling,
grid/history copy, dependency or feature is added to production.

Alacritty already allows arbitrarily long mark vectors in its authoritative
model. Additional paint allocation/shaping is limited to the base plus 32
combining scalars per cell. Pathological longer mark stacks are explicitly
unsupported for complete display, while selection/copy retains the full model
text. No text outside the model is fabricated and the model is never truncated.

## Fixtures and verification

Seven focused adapter regressions use deterministic VT models to check literal
CJK/ASCII/combining positions and cursor columns, leading wrap padding,
exact-row-end wide cells, narrow reflow, authoritative selection strings,
continuation/hidden handling, bounded mark stacks and unchanged-generation
cache reuse. These run in the explicit vendor suite; application tests alone
do not run them.

The opt-in `unicode_cells` example uses a real PTY/ConPTY synthetic child and
actual widget renderer, with two installed monospace families: Menlo/Courier
New on macOS, Consolas/Courier New on Windows, plus system fallback fonts.
It captures 100%, 125%, 150% and 200% offscreen scales and compares:

- ASCII sentinels beside CJK and accents against fixed-column reference rows.
- Decomposed `e + U+0301` against precomposed `é`, and the lower base glyph
  against unaccented `e` so two equally clipped glyphs cannot pass. Reference
  ASCII rows also detect pathological accent ink escaping its own row.
- Distinct CJK glyphs to reject identical missing-glyph boxes.
- Combining overlays and a wide cursor against adjacent ASCII cell crops.
- Cursor placement on a wide lead versus its continuation.
- The actual Copy binding with a synthetic clipboard, before and after wide
  wrapping at eight/six columns, without copying continuation padding.

The existing backend copy path delegates to Alacritty `selection_to_string`;
no #15 clipboard extraction feature is duplicated. Six/eight-column native
scenarios redraw their controlled fixture after resize; the independent VT
model test covers preservation during actual reflow. Screenshots contain only
known fixture text. Child command echo is disabled in the child so control
commands cannot corrupt the grid. All exits require backend completion and
the native wrapper checks owned child PIDs are absent.

`unicode-cells.yml` follows the existing R13 workflow: exact PR SHA checkout,
SHA-pinned official actions, read-only permissions, four build jobs, locked
dependencies, an isolated target directory and synthetic artifacts with source,
compiler, OS, runtime and executable hashes. It runs explicit adapter tests and
both renderer release probes natively on Windows Server 2025. It does not claim
physical monitor transitions or keyboard input coverage.

Check results and measurements will be appended after execution. No native
Windows or resource-budget pass is inferred from macOS/cross compilation.

## Explicit limitations

The emulator's cells define terminal width. Contextual shaping across separate
cells, arbitrary ZWJ/family emoji ligatures, regional-indicator grouping,
variation-selector width negotiation and reliable color-emoji appearance are
not promised. The fixture includes `man + ZWJ + woman` and a 40-mark stack as
known unsupported examples; these remain clipped to model cell spans and do
not displace adjacent text. A font still needs an appropriate fallback glyph;
the renderer does not synthesize missing CJK or box-drawing outlines. Very
tall accents or oversized italic/fallback ink can be cropped at cell edges to
preserve neighboring cells. Copy retains authoritative Unicode even where a
complex sequence's appearance is unsupported. This does not add IME (#19),
keyboard protocol (#14/#45), bidi reordering or a new grapheme-width algorithm.
