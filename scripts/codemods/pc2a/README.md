# PC-2a: wrapper calls to craft_ui components

Rewrites PhotoCraft's app wrapper calls (`crates/ui-egui/src`) into `craft_ui::controls` components. Python 3 standard library only; no build needed.

```
python3 -I scripts/codemods/pc2a/pc2a.py --root <photocraft checkout>            # dry run (default), prints totals
python3 -I scripts/codemods/pc2a/pc2a.py --root . --diff out.diff --report r.md  # also write a diff and the markdown report
python3 -I scripts/codemods/pc2a/pc2a.py --root . --apply                        # rewrite in place
```

- Matches syntax, not line numbers: comments and string literals are masked, call arguments are split at top-level commas. Test files, `tests/`, `compact_visual_regressions.rs` and inline `#[cfg(test)] mod` blocks are skipped.
- Idempotent: a rewritten call no longer matches. Run again after `--apply` to catch calls that were nested inside another rewritten call.
- Output uses fully qualified `craft_ui::...` paths, so no `use` lines change. Run `cargo fmt --all` afterwards; long lines stay unwrapped.
- `--craft-libs DIR` (default `$CRAFT_LIBS`, else `../craft-libs`) and `--craft-libs-ref REF` (default `origin/design-system`) name the manifest read with `git show REF:crates/ui/icons/icons.toml` to turn icon names into `Icon::Variant`. Names not in the manifest are left and listed. `--source-commit SHA` names the PhotoCraft commit in the report when the checkout's HEAD carries only the scripts.

## Rewrites

| Call | Becomes |
|---|---|
| `checkbox(ui, &mut b, label)` | `Checkbox::bool(tl!(label), &mut b).show(ui).response` |
| `widgets::toggle(ui, &mut b, label)` | `Switch::new(tl!(label), &mut b).show(ui).response` |
| `primary_button` / `secondary_button(ui, label, w)` | `ui.add(Button::new(tl!(label)).variant(..).min_width(w))` (`min_width` omitted for `0.0`) |
| `icons::button*(ui, "name", box, selected, tip[, show_tip])` | `ui.add(IconButton::new(Icon::Name, tip).size(Sm\|Md)[.selected(sel)][.show_tooltip(b)])` (box 16-22 px Sm, 24-34 px Md; icon names via the DS-43 alias table, only when the manifest lists the target) |
| `if icons::button*(ui, "n", box, P, tip).clicked() { P = !P; }` | `ui.add(IconButton::new(Icon::N, tip).size(..).toggle(&mut P));` (P a plain path or `*name`) |
| `dialog_buttons(ui, &[..] \| &row)` | `FooterButtons::new("dialog_buttons", vec![..] \| row.to_vec()).show(ui).clicked.map(\|c\| c.role)` (`craft_ui::dialog`); only when every button in the row is built by a rewritable `DialogButton::new` |
| `DialogButton::new(ButtonRole::X, label, w)` | `FooterButton::new(craft_ui::dialog::ButtonRole::X, tl!(label)).min_width(w)` (`.enabled(..)` chains on; a `.focus(..)` chain or a non-literal role is left) |
| `section_label(ui, text);` | `SectionLabel::new(tl!(text)).show(ui);` (statement position only) |
| `panel_footer(ui, \|bar\| ..)` | `PanelFooter::show(ui, \|bar\| ..)` (same signature; only the callee changes) |
| `compact_button(ui, label, h)` | `ui.add(Button::new(tl!(label)).size(Sm))` (height dropped, DS-43 A11) |
| `dropdown` / `dropdown_literal` / `dropdown_with_tooltips(ui, id, name, &mut cur, options, w)` | `Select::new(id, name, &mut cur).fit(true).option(..)[.option_tooltip(..)].show(ui).response.changed()`; a path of pairs becomes `.rows(..)` without `.fit` |

`tl!` wraps a label that is not already `tl!(..)`, because the old widgets translated every label.

Everything else is counted and left, with the reason in the report: `value_field*` and `slider*` including `slider_row*` (`NumberField`, `Slider` and `SliderRow` report `EditEvent`s while the callers use the returned `Response` of a `&mut f32`: they need the app-side `bound.rs` adapter), `text_field*`, `pill_tab`, `rail_button` (the dock's iconic column replaces it), `dropdown_hovered`/`_wheel_hovered`, path-form `dropdown_with_tooltips`, icon buttons with a non-literal name or box size, and menus.

## Checks before committing a rewrite

- The dialog rewrites return craft-ui's `ButtonRole`; imports of `widgets::ButtonRole` and `DialogButton` in those files change with them. Whole-dialog `Footer` (inside `dialog::Dialog`) is a port decision, not a call rewrite.
- Needs the craft-libs pin that has these controls; the codemod never touches `Cargo.toml`.
- `Select::rows` closures borrow the options slice; a site where the slice borrows the selected value needs a hand fix (compiler error).
- Select ids are passed as given (`&str` or `&format!(..)`); confirm `AsIdSalt` accepts them.
