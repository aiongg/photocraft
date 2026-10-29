# PC-2b: token reads and literals onto craft-libs design-system tokens

`pc2b.py` rewrites, in `crates/ui-egui/src`, only mappings that are exact:

| Pattern | Rewrite | Left (recorded with a reason) |
|---|---|---|
| `t.<field>` (`t`, `tokens`, `theme`, `tk`) | `t.color.<role>.to_egui()`, `t.component.<...>.to_egui()`, `t.radius.control/popup/window` | `icon`, `accent_border`, `canvas_dot`, `shadow`, `histogram_level`; assignments; receivers not bound to `Tokens` in the file; `radius*` used as f32 |
| `Tokens::get(<ui>.ctx())` | `Tokens::of(<ui>)` | `ctx`, `&ctx`, `painter.ctx()` |
| `add_space(<literal>)` | `craft_theme::Space::Sn.px()` | off-scale (nearest step and difference), negative, non-literal |
| `Stroke::new(<literal>, ..)` | `craft_theme::Stroke::W1/W1_5/W2.width(1.0)` | off-scale, zero, non-literal |
| `.rect_filled/.rect_stroke(r, <literal>, ..)` | `craft_theme::Radius::Rn.px()` | off-scale, non-literal |
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

Options: `--root DIR` (checkout, default: the one holding the script), `--src` (default `crates/ui-egui/src`), `--scale PATH` (module path of `Space`/`Radius`/`Stroke` in emitted code, default `craft_theme`; `craft_ui::theme` once craft-ui re-exports them), `--json FILE`.

After `--apply`, the file gets `use craft_ui::theme::ToEgui;` where it needed it. The tree compiles only after the left sites and the `craft-theme`/re-export question are settled; see "Missing in craft-libs" and "Notes" in the report (`.thoughts/theming/design-system/pc0/codemod-reports/pc2b.md`).
