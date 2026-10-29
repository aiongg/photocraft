# PC-2b: token reads and literals onto craft-libs design-system tokens

`pc2b.py` rewrites, in `crates/ui-egui/src`, only mappings that are exact:

| Pattern | Rewrite | Left (recorded with a reason) |
|---|---|---|
| `t.<field>` (`t`, `tokens`, `theme`, `tk`) | `t.color.<role>.to_egui()`, `t.component.<...>.to_egui()`, `t.shape.radius.control/popup/window` (f32) | `accent_border`, `canvas_dot`, `shadow`, `histogram_level` (decided as per-use or app work in DS-43); assignments; receivers not bound to `Tokens` in the file;  |
| `Tokens::get(<ui>.ctx())` | `Tokens::of(<ui>)`; any other Context: `craft_ui::theme::current(<ctx>)` | none |
| `add_space(<literal>)` | `craft_ui::theme::scale::Space::Sn.px()` (Space 3, 5, 7, 9, 10, 14, 18, 22, 26, 46 are rounded per DS-43 B8; `--exact-only` skips these) | off-scale (nearest step and difference), negative, non-literal |
| `Stroke::new(<literal>, ..)` | `scale::Stroke::W1/W1_5/W2.egui(<ctx>, <colour>)` | off-scale, zero, non-literal |
| `.rect_filled/.rect_stroke(r, <literal>, ..)` | `craft_ui::theme::scale::Radius::Rn.px()` (1.0 rounds to R2, DS-43 B9) | off-scale, non-literal |
| `CornerRadius::same(n)`, `.corner_radius(n)` | none (u8 API) | all |

Field mapping is `COLOR_FIELDS` in the script (from themes.md Table 0).

Excluded: `*_tests.rs`, `tests.rs`, `tests/`, modules declared `#[cfg(test)] mod x;`, inline `#[cfg(test)] mod x { .. }` blocks (cut to end of file). `theme.rs` and `theme/` are counted but never rewritten. Comments and string contents are masked before matching. Matching is on syntax, so re-runs after a rebase work, and a second `--apply` changes nothing (converted sites are counted as "already converted").

## Usage

From the checkout root:

```
python3 -I scripts/codemods/pc2b/pc2b.py                      # --dry-run (default): writes nothing to source
python3 -I scripts/codemods/pc2b/pc2b.py --report FILE.md     # also write the markdown report
python3 -I scripts/codemods/pc2b/pc2b.py --dump-left FILE.tsv # every left site: pattern, file:line, reason
python3 -I scripts/codemods/pc2b/pc2b.py --apply              # rewrite exact matches in place
```

Options: `--root DIR` (checkout, default: the one holding the script), `--src` (default `crates/ui-egui/src`), `--scale PATH` (module path of `Space`/`Radius`/`Stroke` in emitted code, default `craft_ui::theme::scale`), `--json FILE`, `--exact-only` (skip the B8/B9 roundings), `--source-commit SHA` (commit named in the report).

After `--apply`, the file gets `use craft_ui::theme::ToEgui;` where it needed it. The tree compiles only after the left sites and the `craft-theme`/re-export question are settled; see "Missing in craft-libs" and "Notes" in the report (`.thoughts/theming/design-system/pc0/codemod-reports/pc2b.md`).

Role tokens: `add_space(n)` and `item_spacing.x/y = n` get `t.space.gap.section|stack|inline` instead of an S-step when the neighbouring statements show the role (separator or heading beside it, a gap between two rows, the axis of `item_spacing`) and a `Tokens` value is bound in the same fn. Literals that differ from the token in Compact or Comfortable are listed as "shifted" in the report.
