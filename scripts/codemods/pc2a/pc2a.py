#!/usr/bin/env python3
"""PC-2a codemod: PhotoCraft app wrapper calls -> craft_ui components.

Usage: python3 -I pc2a.py [--root DIR] [--apply] [--diff FILE] [--report FILE]
See README.md. Matches on syntax (comments and strings are masked), never on line numbers,
and a rewritten call no longer matches, so re-runs are idempotent.
"""
import argparse
import collections
import datetime
import difflib
import json
import os
import re
import subprocess
import sys

CU = "craft_ui::controls::"
SRC_REL = "crates/ui-egui/src"

# ---------------------------------------------------------------- source scanning


def mask(src):
    """Copy of src with comments and string/char literal contents replaced by spaces."""
    out = list(src)
    n, i = len(src), 0

    def blank(a, b):
        for k in range(a, b):
            if out[k] != "\n":
                out[k] = " "

    while i < n:
        c = src[i]
        if src.startswith("//", i):
            j = src.find("\n", i)
            j = n if j < 0 else j
            blank(i, j)
            i = j
        elif src.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if src.startswith("/*", j):
                    depth += 1
                    j += 2
                elif src.startswith("*/", j):
                    depth -= 1
                    j += 2
                else:
                    j += 1
            blank(i, j)
            i = j
        elif c == "r" and re.match(r'r#*"', src[i:i + 12]) and (i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_")):
            m = re.match(r'r(#*)"', src[i:])
            end = '"' + m.group(1)
            j = src.find(end, i + len(m.group(0)))
            j = n if j < 0 else j + len(end)
            blank(i + len(m.group(0)), j - len(end))
            i = j
        elif c == '"':
            j = i + 1
            while j < n and src[j] != '"':
                j += 2 if src[j] == "\\" else 1
            blank(i + 1, j)
            i = j + 1
        elif c == "'":
            m = re.match(r"'(\\.[^']*|[^\\'])'", src[i:i + 12])
            if m:
                blank(i + 1, i + len(m.group(0)) - 1)
                i += len(m.group(0))
            else:
                i += 1
        else:
            i += 1
    return "".join(out)


def split_args(m, start):
    """m = masked text, start = index just after '('. Returns (arg spans, end index after ')') or None."""
    depth, i, a, spans = 0, start, start, []
    while i < len(m):
        c = m[i]
        if c in "([{":
            depth += 1
        elif c in ")]}":
            if depth == 0:
                if m[a:i].strip():
                    spans.append((a, i))
                return spans, i + 1
            depth -= 1
        elif c == "," and depth == 0:
            spans.append((a, i))
            a = i + 1
        elif c == "|" and depth == 0:
            return None  # closure at top level: do not guess
        i += 1
    return None


TEST_MOD = re.compile(r"#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{")


def is_test_file(rel):
    base = os.path.basename(rel)
    return base.endswith("_tests.rs") or base == "tests.rs" or "/tests/" in "/" + rel or base == "compact_visual_regressions.rs"


# ---------------------------------------------------------------- icons manifest

# PhotoCraft icon file name -> manifest name (DS-43 "Icon manifest additions"). A name is used
# only when the manifest at the craft-libs ref lists it; the others are reported as missing.
ALIASES = {
    "x": "close", "undo-2": "undo", "ellipsis": "more", "pipette": "eyedropper", "file-plus": "new_file",
    "link-2": "link", "message-square": "note", "square-plus": "new_layer", "folder-plus": "new_folder",
    "arrow-left-right": "swap_horizontal", "circle-dot": "target", "sliders-horizontal": "adjustments",
    "grid-3x3": "grid", "grid-2x2": "grid_small", "square-dashed": "marquee_rect",
    "circle-dashed": "marquee_ellipse", "house": "home", "rectangle-vertical": "orientation_portrait",
    "rectangle-horizontal": "orientation_landscape", "layer-mask": "layer_mask", "font-variable": "font_variable",
    "font-similar": "font_similar", "painting-symmetry": "symmetry", "text-warp": "text_warp",
}


DEFAULT_REF = "origin/design-system"


def load_icons(craft_libs, ref=DEFAULT_REF):
    """manifest names -> Icon variant, or None when the manifest cannot be read."""
    try:
        t = subprocess.run(["git", "-C", craft_libs, "show", f"{ref}:crates/ui/icons/icons.toml"],
                           capture_output=True, text=True, check=True).stdout
    except Exception:
        return None
    names = re.findall(r'^name\s*=\s*"([^"]+)"', t, re.M)
    return {n: "".join(w[:1].upper() + w[1:] for w in n.split("_")) for n in names}


def resolve_ref(craft_libs, ref):
    """Short hash of ref in the craft-libs checkout, or the ref text when git cannot say."""
    try:
        return subprocess.run(["git", "-C", craft_libs, "rev-parse", "--short=8", ref], capture_output=True, text=True, check=True).stdout.strip()
    except Exception:
        return ref


def icon_variant(file_name, icons):
    """(variant, None) or (None, manifest name wanted)."""
    plain = file_name.replace("-", "_")
    want = ALIASES.get(file_name, plain)
    if want not in icons and plain in icons:
        want = plain  # the manifest has since added the icon under its own name
    return (icons[want], None) if want in icons else (None, want)


# ---------------------------------------------------------------- rewrite rules
# Each rule: name -> (regex for the callee, function(ctx) -> (replacement | None, reason))

PRE = r"(?<![\w.:])((?:crate::|super::|self::)*(?:widgets::|icons::)?)"
RULES = {}


def rule(label, names, qualifier=None, bare=True):
    """Register a pattern. qualifier: required path segment ('widgets'/'icons') or None."""
    alt = "|".join(names)
    RULES[label] = dict(re=re.compile(PRE + r"(" + alt + r")\s*\(") , qual=qualifier, bare=bare)


rule("checkbox", ["checkbox"], "widgets")
rule("toggle", ["toggle"], "widgets", bare=False)
rule("icons::button", ["button", "button_with_icon_size", "button_with_icon_size_tip", "rail_button"], "icons", bare=False)
rule("dropdown", ["dropdown", "dropdown_literal", "dropdown_hovered", "dropdown_wheel_hovered", "dropdown_with_tooltips"], "widgets")
rule("value_field", ["value_field", "value_field_named", "value_field_prec", "value_field_units", "value_field_extent",
                     "compact_value_field", "compact_value_field_prec", "number_edit", "popup_value_field"], "widgets")
rule("primary_button", ["primary_button"], "widgets")
rule("secondary_button", ["secondary_button"], "widgets")
rule("compact_button", ["compact_button"], "widgets")
rule("slider", ["slider", "slider_row", "slider_row_with", "slider_row_prec"], "widgets")
rule("pill_tab", ["pill_tab", "pill_tabs"], "widgets")
rule("text_field", ["text_field", "text_field_ex"], "widgets")
rule("dialog_buttons", ["dialog_buttons"], "widgets")
rule("DialogButton::new", [r"DialogButton::new"], None)
rule("section_label", ["section_label"], "widgets")
rule("panel_footer", ["panel_footer"], "widgets")
rule("radio", ["radio", "radio_value"], "widgets", bare=False)
rule("panel_menu", ["panel_menu_button", "panel_menu"], "widgets")
rule("menu_scroll", ["menu_scroll"], "widgets")
rule("style_spectrum_popup_menu", ["style_spectrum_popup_menu"], "widgets")

# patterns that are not plain calls: counted only (reason: hand work)
COUNT_ONLY = collections.OrderedDict([
    ("PressCombo", re.compile(r"\bPressCombo\b")),
    ("type_ahead", re.compile(r"\btype_ahead::")),
    ("craft_ui::ComboBox (already ported)", re.compile(r"\bcraft_ui::ComboBox\b")),
    ("Popup::menu", re.compile(r"\bPopup::menu\(")),
    ("context_menu", re.compile(r"\.context_menu\(")),
    ("menu_button", re.compile(r"\.menu_button\(|\bmenu_image_button\b")),
    ("press_menu::show", re.compile(r"\bpress_menu::show\(")),
])

HAND_REASON = {
    "value_field": "needs the app-side adapter `widgets/bound.rs` (host, doc and `apply` for the undo step) and a `&LocaleNumbers`; `value_field` also has no label argument for the accessible name. `NumberField` reports `EditEvent`s, so this is app code, not a rename",
    "slider": "`Slider`/`SliderRow` report `EditEvent`s (Begin/Update/Commit) and take no `&mut f32`; callers use the returned `Response` (`changed`, `drag_stopped`), so the site needs the app-side adapter `widgets/bound.rs` (host, doc, `apply` for the undo step). Arguments map exactly: gradient to `track_colors`, suffix to `Unit`, `decimals` to `SliderRow::decimals`, `RowGestures` to `default_value`/`wheel_step`; `SliderRow` also needs a `&LocaleNumbers`",
    "pill_tab": "one tab per call; DS-43 A12: each group becomes one `Segmented` (2-4 items) or a `TabStrip`, by hand",
    "text_field": "no label argument for the accessible name, and the event's String must be written back; DS-43 A10: hint = `placeholder`, width = layout",
    "dialog_buttons": "argument is neither a `&[..]` literal nor a plain `&name` (the row is built by a helper)",
    "radio": "RadioGroup/RadioSet take the whole group; sites are single rows",
    "panel_menu": "`PanelHeader` (menu button on by default) exists; the site is the dock's own panel menu button, replaced with the dock, not by a call rewrite",
    "menu_scroll": "closure wrapper that Menu makes unnecessary; removal needs the surrounding popup ported first",
    "style_spectrum_popup_menu": "removal is only right once the popup is a craft_ui Menu; until then it styles the egui popup",
}


class Site:
    def __init__(self, rule_, rel, line, text):
        self.rule, self.rel, self.line, self.text = rule_, rel, line, text
        self.status, self.reason, self.new = "left", "", None


def paren(a):
    a = a.strip()
    return a if re.fullmatch(r"[\w.]+", a) else "(" + a + ")"


def ui_expr(a):
    a = a.strip()
    return a if re.fullmatch(r"[\w.]+|\(.*\)", a, re.S) else "(" + a + ")"


def translated(label):
    l = label.strip()
    return l if l.startswith("tl!(") else f"tl!({l})"


def is_zero(a):
    return re.fullmatch(r"0(\.0*)?(_?f32)?", a.strip()) is not None


def plain_path(a):
    return re.fullmatch(r"&?[\w.]+(::\w+)*", a.strip()) is not None


def build(label, name, args, opts, icons, after="", toggle=None):
    """Returns (replacement, None) or (None, reason). after: masked text following the call; toggle: bool path of a flip-on-click site."""
    a = [x.strip() for x in args]
    n = len(a)
    if label == "checkbox":
        if n != 3:
            return None, "unexpected argument count"
        return f"{CU}Checkbox::bool({translated(a[2])}, {a[1]}).show({a[0]}).response", None
    if label == "toggle":
        if n != 3:
            return None, "unexpected argument count"
        return f"{CU}Switch::new({translated(a[2])}, {a[1]}).show({a[0]}).response", None
    if label in ("primary_button", "secondary_button"):
        if n != 3:
            return None, "unexpected argument count"
        v = "Primary" if label == "primary_button" else "Secondary"
        w = "" if is_zero(a[2]) else f".min_width({a[2]})"
        return f"{ui_expr(a[0])}.add({CU}Button::new({translated(a[1])}).variant({CU}ButtonVariant::{v}){w})", None
    if label == "icons::button":
        if name == "rail_button":
            return None, "rail look (quiet filled 'on' state, no accent): the dock's iconic column (`DockLook::iconic_width`, `DockContent::paint_icon`) or `patterns::PanelRail` draws it; not an `IconButton`"
        want = {"button": 5, "button_with_icon_size": 6, "button_with_icon_size_tip": 7}[name]
        if n != want:
            return None, "unexpected argument count"
        nm = re.fullmatch(r'"([\w-]+)"', a[1])
        if not nm:
            return None, "icon name is not a string literal"
        if icons is None:
            return None, "icons manifest not readable"
        var, wanted = icon_variant(nm.group(1), icons)
        if var is None:
            return None, f"MISSING-ICON:{nm.group(1)}={wanted}"
        if not re.fullmatch(r"[\d.]+(_?f32)?", a[2]):
            return None, "box size is not a numeric literal"
        box = float(re.sub(r"[^\d.]", "", a[2]))
        if box > 34 or box < 16:
            return None, "box size outside 16 to 34 px (DS-43 A4 maps 16-22 to Sm, 24-34 to Md)"
        sz = "Sm" if box <= 22 else "Md"
        sel = a[3] if name == "button" else a[4]
        tip = a[4] if name == "button" else a[5]
        if tip in ('""', ""):
            return None, "empty tooltip: IconButton needs a label"
        out = f"{ui_expr(a[0])}.add({CU}IconButton::new(craft_ui::icons::Icon::{var}, {tip}).size({CU}ControlSize::{sz})"
        if toggle:
            out += f".toggle(&mut {toggle})"
        elif sel != "false":
            out += f".selected({sel})"
        if name == "button_with_icon_size_tip" and a[6] != "true":
            out += f".show_tooltip({a[6]})"
        return out + ")", None
    if label == "dropdown":
        if name == "dropdown_hovered":
            return None, "returns (changed, hovered); write `.highlighted(&mut out)` by hand"
        if name == "dropdown_wheel_hovered":
            return None, "returns every option chosen per notch plus the hovered one; port uses `.wheel_steps(true)` and `.highlighted(..)` by hand (DS-43 A2)"
        if n != 6:
            return None, "unexpected argument count"
        tooltips = name == "dropdown_with_tooltips"
        tr = name != "dropdown_literal"
        o = a[4]
        lit = re.fullmatch(r"&\[(.*)\]", o, re.S)
        fit = ".fit(true)"
        if lit:
            chain = ""
            for it in split_top(lit.group(1)):
                m = re.fullmatch(r"\((.*)\)", it.strip(), re.S)
                parts = split_top(m.group(1)) if m else []
                if len(parts) != (3 if tooltips else 2):
                    return None, "option list literal has a non-tuple element"
                t = parts[1].strip()
                chain += f".option({parts[0].strip()}, {translated(t) if tr else t})"
                if tooltips:
                    chain += f".option_tooltip({translated(parts[2].strip())})"
        elif plain_path(o) and not tooltips:
            ref = o.lstrip("&")
            txt = f"tl!({ref}[i].1)" if tr else f"{ref}[i].1"
            chain = f".rows({ref}.len(), |i| {ref}[i].0.clone(), |i| {txt})"
            fit = ""  # fit is ignored with rows()
        elif tooltips:
            return None, "options is a path with per-option tooltips: `rows` has no tooltip; write the `.option(..).option_tooltip(..)` chain by hand"
        else:
            return None, "options is an expression, not a literal list or a plain path"
        return f"{CU}Select::new({a[1]}, {a[2]}, {a[3]}){fit}{chain}.show({a[0]}).response.changed()", None
    if label == "compact_button":
        if n != 3:
            return None, "unexpected argument count"
        return f"{ui_expr(a[0])}.add({CU}Button::new({translated(a[1])}).size({CU}ControlSize::Sm))", None
    if label == "DialogButton::new":
        if n != 3:
            return None, "unexpected argument count"
        role = re.fullmatch(r"(?:\w+::)*ButtonRole::(Default|Alternate|Cancel|Apply)", a[0])
        if not role:
            return None, "role is not a `ButtonRole::X` literal: the row is built from PhotoCraft's own `ButtonRole` values"
        if after.lstrip().startswith(".focus("):
            return None, "`.focus(..)`: FooterButtons moves focus by arrow keys; the initial focus is the dialog's (`InitialFocus`)"
        w = "" if is_zero(a[2]) else f".min_width({a[2]})"
        return f"craft_ui::dialog::FooterButton::new(craft_ui::dialog::ButtonRole::{role.group(1)}, {translated(a[1])}){w}", None
    if label == "section_label":
        if n != 2:
            return None, "unexpected argument count"
        if not after.lstrip().startswith(";"):
            return None, "call is used as an expression (SectionLabel::show returns a Response)"
        return f"craft_ui::composites::SectionLabel::new({translated(a[1])}).show({a[0]})", None
    if label in HAND_REASON:
        return None, HAND_REASON[label]
    return None, "no rule"


def split_top(s):
    depth, parts, a = 0, [], 0
    m = mask(s)
    for i, c in enumerate(m):
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
        elif c == "," and depth == 0:
            parts.append(s[a:i])
            a = i + 1
    if s[a:].strip():
        parts.append(s[a:])
    return parts


def toggle_site(m, start, end, sel, after):
    """(path, span start, span end) when the call is `if <call>.clicked() { P = !P; }` with P the selected argument."""
    sel = sel.strip()
    if not re.fullmatch(r"\*?[A-Za-z_][\w.]*", sel):
        return None, start, end
    tail = re.match(r"\.clicked\(\)\s*\{\s*(\*?[A-Za-z_][\w.]*)\s*=\s*!\s*(\*?[A-Za-z_][\w.]*)\s*;\s*\}", after)
    head = re.search(r"(?<![\w.])if\s+$", m[max(0, start - 40):start])
    if not tail or not head or tail.group(1) != sel or tail.group(2) != sel:
        return None, start, end
    if re.search(r"\belse\s+$", m[max(0, start - 60):start - len(head.group(0))]):
        return None, start, end
    path = sel.lstrip("*")
    return path, start - len(head.group(0)), end + tail.end()


BAD_BUTTON = re.compile(r"DialogButton::new\(\s*(?!(?:\w+::)*ButtonRole::(?:Default|Alternate|Cancel|Apply)\b)|\.focus\(")


def binding_ok(m, name, call_start):
    """True when `let name` is bound shortly before the call and its DialogButton::new calls all have an exact rewrite."""
    lets = list(re.finditer(rf"\blet\s+(?:mut\s+)?{re.escape(name)}\b", m[max(0, call_start - 6000):call_start]))
    if not lets:
        return False
    seg = m[max(0, call_start - 6000) + lets[-1].start():call_start]
    return "DialogButton::new" in seg and not BAD_BUTTON.search(seg)


def dialog_row_edits(src, spans, end, call_start, args):
    """Edits that turn `dialog_buttons(ui, ROW)` into a FooterButtons row, or (None, reason). ROW is `&[..]` or `&name`."""
    if len(args) != 2:
        return None, "unexpected argument count"
    sa, sb = spans[1]
    text = src[sa:sb]
    lead, trail = len(text) - len(text.lstrip()), len(text) - len(text.rstrip())
    a0, b0 = sa + lead, sb - trail
    row = src[a0:b0]
    head = f'craft_ui::dialog::FooterButtons::new("dialog_buttons", '
    tail = f".show({args[0].strip()}).clicked.map(|c| c.role)"
    m = mask(src)
    idents = [x for x in split_top(row[2:-1]) if re.fullmatch(r"\s*[A-Za-z_]\w*\s*", x)] if row.startswith("&[") and row.endswith("]") else []
    if re.fullmatch(r"&[A-Za-z_][\w.]*", row):
        idents = [row[1:]]
    for x in idents:
        if not binding_ok(m, x.strip().split(".")[0], call_start):
            return None, f"`{x.strip()}` is built elsewhere (or with a role/`.focus` that has no exact rewrite): hand work"
    if BAD_BUTTON.search(mask(row)):
        return None, "a button in the row has a non-literal role or `.focus(..)`: hand work"
    if row.startswith("&[") and row.endswith("]"):
        return [(call_start, a0 + 2, head + "vec!["), (b0 - 1, end, "])" + tail)], None
    if re.fullmatch(r"&[A-Za-z_][\w.]*", row):
        return [(call_start, a0 + 1, head), (b0, end, ".to_vec())" + tail)], None
    return None, HAND_REASON["dialog_buttons"]


# ---------------------------------------------------------------- per-file processing


def process(rel, src, opts, icons, sites, counts_only):
    m = mask(src)
    cut = TEST_MOD.search(m)
    limit = cut.start() if cut else len(src)
    local_fns = set(re.findall(r"\bfn\s+(\w+)", m))
    edits = []
    for label, r in RULES.items():
        for mt in r["re"].finditer(m, 0, limit):
            prefix, name = mt.group(1), mt.group(2)
            if re.search(r"\bfn\s+$", m[max(0, mt.start() - 12):mt.start()]) or m[max(0, mt.start() - 4):mt.start()].endswith("fn "):
                continue
            if not prefix and not r["bare"]:
                continue
            if prefix and r["qual"] and r["qual"] not in prefix:
                continue
            if not prefix and name in local_fns and not rel.startswith("widgets/") and rel != "widgets.rs":
                continue  # a local function of the same name, not the wrapper
            end0 = mt.end()
            line = src.count("\n", 0, mt.start()) + 1
            if label == "panel_footer":
                # same signature as PanelFooter::show; the closure argument stops argument parsing, so only the callee changes
                site = Site(label, rel, line, src[mt.start():mt.start() + 90].split("\n")[0])
                sites.append(site)
                site.status, site.new = "rewritten", "craft_ui::composites::PanelFooter::show("
                edits.append((mt.start(), end0, site.new, site))
                continue
            parsed = split_args(m, end0)
            site = Site(label, rel, line, src[mt.start():mt.start() + 90].split("\n")[0])
            sites.append(site)
            if not parsed:
                site.reason = "arguments not parseable (closure at top level)"
                continue
            spans, end = parsed
            args = [src[a:b] for a, b in spans]
            if label == "dialog_buttons":
                multi, why = dialog_row_edits(src, spans, end, mt.start(), args)
                if multi is None:
                    site.reason = why
                    continue
                site.status, site.new = "rewritten", "craft_ui::dialog::FooterButtons::new(..).show(ui).clicked.map(|c| c.role)"
                edits.extend((a, b, t, site) for a, b, t in multi)
                continue
            after = m[end:end + 120]
            toggle, tstart, tend = None, mt.start(), end
            if label == "icons::button" and len(args) >= 5:
                toggle, tstart, tend = toggle_site(m, mt.start(), end, args[3] if name == "button" else args[4], after)
            new, why = build(label, name, args, opts, icons, after=after, toggle=toggle)
            if new is None:
                site.reason = why
                continue
            if toggle:
                new += ";"
                site.reason = "flip-on-click folded into IconButton::toggle"
            site.status, site.new = "rewritten", new
            edits.append((tstart, tend, new, site))
    for label, rx in COUNT_ONLY.items():
        for mt in rx.finditer(m, 0, limit):
            line = src.count("\n", 0, mt.start()) + 1
            s = Site(label, rel, line, src[mt.start():mt.start() + 90].split("\n")[0])
            s.reason = "hand work: menu/select behaviour is PhotoCraft's own press-menu logic" if label != "craft_ui::ComboBox (already ported)" else "already a craft_ui control"
            counts_only.append(s)
    # drop nested edits (rerun handles them)
    edits.sort(key=lambda e: e[0])
    kept, last = [], -1
    for e in edits:
        if e[0] < last:
            e[3].status, e[3].reason, e[3].new = "left", "nested inside another rewritten call (rerun after --apply)", None
            continue
        kept.append(e)
        last = e[1]
    out, pos = [], 0
    for s0, e0, new, _ in kept:
        out.append(src[pos:s0])
        out.append(new)
        pos = e0
    out.append(src[pos:])
    return "".join(out)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--root", default=".", help="PhotoCraft checkout (default: cwd)")
    ap.add_argument("--apply", action="store_true", help="rewrite files in place (default: dry run)")
    ap.add_argument("--craft-libs", default=os.environ.get("CRAFT_LIBS", "../craft-libs"),
                    help="craft-libs checkout with an origin/design-system ref (default: $CRAFT_LIBS or ../craft-libs)")
    ap.add_argument("--craft-libs-ref", default=DEFAULT_REF, help=f"craft-libs ref the icon manifest is read from (default: {DEFAULT_REF})")
    ap.add_argument("--source-commit", help="PhotoCraft commit named in the report (default: HEAD of the checkout)")
    ap.add_argument("--diff", help="write a unified diff of the rewrites to FILE")
    ap.add_argument("--report", help="write the markdown report to FILE")
    ap.add_argument("--json", help="write every site as JSON to FILE")
    opts = ap.parse_args()
    root = os.path.abspath(opts.root)
    base = os.path.join(root, SRC_REL)
    icons = load_icons(opts.craft_libs, opts.craft_libs_ref)
    sites, counts_only, diffs, nfiles, changed_files = [], [], [], 0, 0
    for d, _, files in os.walk(base):
        for f in sorted(files):
            if not f.endswith(".rs"):
                continue
            path = os.path.join(d, f)
            rel = os.path.relpath(path, base)
            if is_test_file(rel):
                continue
            nfiles += 1
            src = open(path, encoding="utf-8").read()
            new = process(rel, src, opts, icons, sites, counts_only)
            if new != src:
                changed_files += 1
                diffs.extend(difflib.unified_diff(src.splitlines(True), new.splitlines(True), f"a/{SRC_REL}/{rel}", f"b/{SRC_REL}/{rel}"))
                if opts.apply:
                    open(path, "w", encoding="utf-8").write(new)
    if opts.diff:
        open(opts.diff, "w").write("".join(diffs))
    if opts.json:
        json.dump([s.__dict__ for s in sites + counts_only], open(opts.json, "w"), indent=1)
    summary(sites, counts_only, nfiles, changed_files, opts, root)
    if opts.report:
        report(sites, counts_only, nfiles, changed_files, opts, root, icons)


def commit(root):
    try:
        return subprocess.run(["git", "-C", root, "rev-parse", "--short=10", "HEAD"], capture_output=True, text=True).stdout.strip()
    except Exception:
        return "unknown"


def group(sites):
    g = collections.OrderedDict()
    for s in sites:
        g.setdefault(s.rule, []).append(s)
    return g


def reasons(ss):
    c = collections.OrderedDict()
    for s in ss:
        if s.status == "left":
            key = "icon not in the craft-libs manifest yet" if s.reason.startswith("MISSING-ICON") else s.reason
            c.setdefault(key, []).append(s)
    return c


def summary(sites, counts_only, nfiles, changed, opts, root):
    mode = "applied" if opts.apply else "dry run"
    print(f"{mode}: {nfiles} files scanned, {changed} would change" if not opts.apply else f"{mode}: {nfiles} files scanned, {changed} changed")
    for rule_, ss in group(sites).items():
        print(f"{rule_:28} found {len(ss):4}  rewritten {sum(s.status == 'rewritten' for s in ss):4}")
    for rule_, ss in group(counts_only).items():
        print(f"{rule_:36} {len(ss):4} (hand)")


def report(sites, counts_only, nfiles, changed, opts, root, icons=None):
    L = []
    L.append("# PC-2a dry-run report: wrapper calls to craft-libs components\n")
    L.append(f"- Run date: {datetime.date.today().isoformat()}")
    L.append(f"- Source: PhotoCraft `{opts.source_commit or commit(root)}`, `{SRC_REL}` ({nfiles} non-test `.rs` files; test files and inline `#[cfg(test)] mod` blocks excluded)")
    L.append(f"- Mode: dry run{' '}; script `scripts/codemods/pc2a/pc2a.py`")
    short = resolve_ref(opts.craft_libs, opts.craft_libs_ref)
    L.append(f"- craft-libs: `{short}`" + ("" if short.startswith(opts.craft_libs_ref) else f" (`{opts.craft_libs_ref}`)") + (f" ({len(icons)} icons in the manifest)" if icons else " (icon manifest not readable)"))
    L.append(f"- Files that would change: {changed}\n")
    L.append("## Totals\n")
    L.append("| Pattern | Found | Rewritten | Left for hand work |")
    L.append("|---|---|---|---|")
    for rule_, ss in group(sites).items():
        w = sum(s.status == "rewritten" for s in ss)
        L.append(f"| `{rule_}` | {len(ss)} | {w} | {len(ss) - w} |")
    for rule_, ss in group(counts_only).items():
        L.append(f"| `{rule_}` | {len(ss)} | 0 | {len(ss)} |")
    L.append("")
    for rule_, ss in group(sites).items():
        L.append(f"## `{rule_}`\n")
        w = [s for s in ss if s.status == "rewritten"]
        L.append(f"Found {len(ss)}, rewritten {len(w)}.\n")
        if w:
            L.append("Example rewrite: `" + w[0].text.strip()[:80].replace("|", "\\|") + "` at " + f"`{w[0].rel}:{w[0].line}`\n")
        rs = reasons(ss)
        if rs:
            L.append("| Left for hand work: reason | Sites | Examples |")
            L.append("|---|---|---|")
            for why, xs in sorted(rs.items(), key=lambda kv: -len(kv[1])):
                ex = ", ".join(f"`{x.rel}:{x.line}`" for x in xs[:3])
                L.append(f"| {why.replace('|', chr(92) + '|')} | {len(xs)} | {ex} |")
            L.append("")
    L.append("## Menu and select sites (counted, not rewritten)\n")
    L.append("| Pattern | Sites | Examples |")
    L.append("|---|---|---|")
    for rule_, ss in group(counts_only).items():
        L.append(f"| `{rule_}` | {len(ss)} | " + ", ".join(f"`{x.rel}:{x.line}`" for x in ss[:3]) + " |")
    L.append("")
    miss = collections.Counter(s.reason.split(":", 1)[1] for s in sites if s.reason.startswith("MISSING-ICON"))
    L.append("## Icon names not in the craft-libs manifest\n")
    if miss:
        L.append("PhotoCraft name, manifest name it needs (count):\n")
        L.append(", ".join(f"`{k.replace('=', '` -> `')}` ({v})" for k, v in miss.most_common()) + "\n")
    else:
        L.append("None.\n")
    open(opts.report, "w").write("\n".join(L) + "\n")


if __name__ == "__main__":
    sys.exit(main())
