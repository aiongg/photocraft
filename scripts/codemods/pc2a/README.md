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
- `--craft-libs DIR` (default `$CRAFT_LIBS`, else `../craft-libs`) is read with `git show origin/design-system:crates/ui/icons/icons.toml` to turn icon names into `Icon::Variant`. Names not in the manifest are left and listed.

## Rewrites

| Call | Becomes |
|---|---|
| `checkbox(ui, &mut b, label)` | `Checkbox::bool(tl!(label), &mut b).show(ui).response` |
| `widgets::toggle(ui, &mut b, label)` | `Switch::new(tl!(label), &mut b).show(ui).response` |
| `primary_button` / `secondary_button(ui, label, w)` | `ui.add(Button::new(tl!(label)).variant(..).min_width(w))` (`min_width` omitted for `0.0`) |
| `icons::button*(ui, "name", box, selected, tip[, show_tip])` | `ui.add(IconButton::new(Icon::Name, tip).size(Sm\|Md)[.selected(sel)][.show_tooltip(b)])` (box 16-22 px Sm, 24-34 px Md; icon names via the DS-43 alias table, only when the manifest lists the target) |
| `compact_button(ui, label, h)` | `ui.add(Button::new(tl!(label)).size(Sm))` (height dropped, DS-43 A11) |
| `dropdown` / `dropdown_literal` / `dropdown_with_tooltips(ui, id, name, &mut cur, options, w)` | `Select::new(id, name, &mut cur).fit(true).option(..)[.option_tooltip(..)].show(ui).response.changed()`; a path of pairs becomes `.rows(..)` without `.fit` |

`tl!` wraps a label that is not already `tl!(..)`, because the old widgets translated every label.

Everything else is counted and left, with the reason in the report: `value_field*` and `slider*` (need the app-side `bound.rs` adapter), `text_field*`, `pill_tab`, `dialog_buttons`, `rail_button`, `dropdown_hovered`/`_wheel_hovered`, path-form `dropdown_with_tooltips`, icon buttons with a non-literal name or box size, and menus.

## Checks before committing a rewrite

- Needs the craft-libs pin that has these controls; the codemod never touches `Cargo.toml`.
- `Select::rows` closures borrow the options slice; a site where the slice borrows the selected value needs a hand fix (compiler error).
- Select ids are passed as given (`&str` or `&format!(..)`); confirm `AsIdSalt` accepts them.
