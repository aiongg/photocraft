#!/usr/bin/env python3
"""PC-2a codemod: PhotoCraft app wrapper calls -> craft_ui components.

Usage: python3 -I pc2a.py [--root DIR] [--apply] [--lossy] [--diff FILE] [--report FILE]
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


def load_icons(craft_libs):
    try:
        t = subprocess.run(["git", "-C", craft_libs, "show", "origin/design-system:crates/ui/icons/icons.toml"],
                           capture_output=True, text=True, check=True).stdout
    except Exception:
        return None
    names = re.findall(r'^name\s*=\s*"([^"]+)"', t, re.M)
    return {n.replace("-", "_"): "".join(w[:1].upper() + w[1:] for w in re.split(r"[_-]", n)) for n in names}


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
    "value_field": "NumberField takes `f64` by value plus `LocaleNumbers` and reports through an EditResponse event; the app passes `&mut f32` with a unit suffix string and pixel width. Needs an adapter, not a rename",
    "slider": "Slider takes the value by value and returns an event; the app passes `&mut f32` with gradient and suffix. Needs an adapter, not a rename",
    "pill_tab": "one tab per call; Segmented takes the whole group (value + items)",
    "text_field": "TextField takes `&str` and reports the new String through an event; IME, hint, char limit and width stay with the caller",
    "dialog_buttons": "DialogFooter (DS-33) is not built in craft-libs",
    "radio": "RadioGroup/RadioSet take the whole group; sites are single rows",
    "panel_menu": "PanelHeader menu (DS-31) is not built; per-panel bodies",
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


def build(label, name, args, opts, icons):
    """Returns (replacement, None) or (None, reason)."""
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
    if label == "compact_button":
        return None, "`height` argument: Button has only ControlSize Sm/Md, no pixel height"
    if label == "icons::button":
        if name == "rail_button":
            return None, "rail look (quiet filled 'on' state, no accent): IconButton has no such state"
        if name == "button_with_icon_size_tip":
            return None, "`show_tip` flag: IconButton has no hidden-tooltip option"
        want = 5 if name == "button" else 6
        if n != want:
            return None, "unexpected argument count"
        nm = re.fullmatch(r'"([\w-]+)"', a[1])
        if not nm:
            return None, "icon name is not a string literal"
        key = nm.group(1).replace("-", "_")
        if icons is None:
            return None, "icons manifest not readable"
        if key not in icons:
            return None, f"MISSING-ICON:{nm.group(1)}"
        sel = a[3] if name == "button" else a[4]
        if sel != "false":
            return None, "`selected` is not literal false: IconButton::toggle takes `&mut bool`, there is no `selected(bool)`"
        if not re.fullmatch(r"[\d.]+(_?f32)?", a[2]):
            return None, "box size is not a numeric literal"
        if not opts.lossy:
            return None, "LOSSY: box size in pixels has no equivalent (ControlSize Sm/Md only)"
        sz = "Sm" if float(re.sub(r"[^\d.]", "", a[2])) <= 24 else "Md"
        tip = a[4] if name == "button" else a[5]
        if tip in ('""', ""):
            return None, "empty tooltip: IconButton needs a label"
        return (f"{ui_expr(a[0])}.add({CU}IconButton::new(craft_ui::icons::Icon::{icons[key]}, {tip})"
                f".size({CU}ControlSize::{sz}))"), None
    if label == "dropdown":
        if name not in ("dropdown", "dropdown_literal"):
            return None, {"dropdown_hovered": "reports the hovered option (live preview): Select has no hover output",
                          "dropdown_wheel_hovered": "wheel steps list plus hover output: Select::wheel_steps has no per-notch list",
                          "dropdown_with_tooltips": "per-option tooltips: Select options have none"}[name]
        if n != 6:
            return None, "unexpected argument count"
        if not opts.lossy:
            return None, "LOSSY: `width` argument has no equivalent (Select fills the row or uses its token width)"
        tr = name == "dropdown"
        o = a[4]
        lit = re.fullmatch(r"&\[(.*)\]", o, re.S)
        if lit:
            items = split_top(lit.group(1))
            chain = ""
            for it in items:
                m = re.fullmatch(r"\((.*)\)", it.strip(), re.S)
                parts = split_top(m.group(1)) if m else []
                if len(parts) != 2:
                    return None, "option list literal has a non-pair element"
                t = parts[1].strip()
                chain += f".option({parts[0].strip()}, {translated(t) if tr else t})"
        elif plain_path(o):
            ref = o.lstrip("&")
            txt = f"tl!({ref}[i].1)" if tr else f"{ref}[i].1"
            chain = f".rows({ref}.len(), |i| {ref}[i].0.clone(), |i| {txt})"
        else:
            return None, "options is an expression, not a literal list or a plain path"
        return f"{CU}Select::new({a[1]}, {a[2]}, {a[3]}){chain}.show({a[0]}).response.changed()", None
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
            if prefix and r["qual"] not in prefix:
                continue
            if not prefix and name in local_fns and not rel.startswith("widgets/") and rel != "widgets.rs":
                continue  # a local function of the same name, not the wrapper
            end0 = mt.end()
            parsed = split_args(m, end0)
            line = src.count("\n", 0, mt.start()) + 1
            site = Site(label, rel, line, src[mt.start():mt.start() + 90].split("\n")[0])
            sites.append(site)
            if not parsed:
                site.reason = "arguments not parseable (closure at top level)"
                continue
            spans, end = parsed
            args = [src[a:b] for a, b in spans]
            new, why = build(label, name, args, opts, icons)
            if new is None:
                site.reason = why
                continue
            site.status, site.new = "rewritten", new
            edits.append((mt.start(), end, new, site))
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
    ap.add_argument("--lossy", action="store_true",
                    help="also rewrite sites that drop a pixel width/size craft_ui cannot express (Select width, IconButton box size)")
    ap.add_argument("--craft-libs", default=os.environ.get("CRAFT_LIBS", "../craft-libs"),
                    help="craft-libs checkout with an origin/design-system ref (default: $CRAFT_LIBS or ../craft-libs)")
    ap.add_argument("--diff", help="write a unified diff of the rewrites to FILE")
    ap.add_argument("--report", help="write the markdown report to FILE")
    ap.add_argument("--json", help="write every site as JSON to FILE")
    opts = ap.parse_args()
    root = os.path.abspath(opts.root)
    base = os.path.join(root, SRC_REL)
    icons = load_icons(opts.craft_libs)
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
        report(sites, counts_only, nfiles, changed_files, opts, root)


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
            key = "icon not in craft-libs manifest" if s.reason.startswith("MISSING-ICON") else s.reason
            c.setdefault(key, []).append(s)
    return c


def summary(sites, counts_only, nfiles, changed, opts, root):
    mode = "applied" if opts.apply else "dry run"
    print(f"{mode}: {nfiles} files scanned, {changed} would change" if not opts.apply else f"{mode}: {nfiles} files scanned, {changed} changed")
    for rule_, ss in group(sites).items():
        print(f"{rule_:28} found {len(ss):4}  rewritten {sum(s.status == 'rewritten' for s in ss):4}")
    for rule_, ss in group(counts_only).items():
        print(f"{rule_:36} {len(ss):4} (hand)")


def report(sites, counts_only, nfiles, changed, opts, root):
    L = []
    L.append("# PC-2a dry-run report: wrapper calls to craft-libs components\n")
    L.append(f"- Run date: {datetime.date.today().isoformat()}")
    L.append(f"- Source: PhotoCraft `{commit(root)}`, `{SRC_REL}` ({nfiles} non-test `.rs` files; test files and inline `#[cfg(test)] mod` blocks excluded)")
    L.append(f"- Mode: dry run{' (--lossy)' if opts.lossy else ''}; script `scripts/codemods/pc2a/pc2a.py`")
    L.append(f"- Files that would change: {changed}\n")
    L.append("## Totals\n")
    L.append("| Pattern | Found | Rewritten | Also rewritten with `--lossy` | Left for hand work |")
    L.append("|---|---|---|---|---|")
    for rule_, ss in group(sites).items():
        w = sum(s.status == "rewritten" for s in ss)
        lo = sum(s.reason.startswith("LOSSY") for s in ss)
        L.append(f"| `{rule_}` | {len(ss)} | {w} | {lo} | {len(ss) - w - lo} |")
    for rule_, ss in group(counts_only).items():
        L.append(f"| `{rule_}` | {len(ss)} | 0 | 0 | {len(ss)} |")
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
                L.append(f"| {why.replace('LOSSY: ', 'eligible with `--lossy`: ').replace('|', chr(92) + '|')} | {len(xs)} | {ex} |")
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
        L.append(", ".join(f"`{k}` ({v})" for k, v in miss.most_common()) + "\n")
    else:
        L.append("None.\n")
    open(opts.report, "w").write("\n".join(L) + "\n")


if __name__ == "__main__":
    sys.exit(main())
