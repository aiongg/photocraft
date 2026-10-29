#!/usr/bin/env python3
"""PC-2b: rewrite PhotoCraft token reads and colour/size literals onto craft-libs design-system tokens.

Run with `python3 -I scripts/codemods/pc2b/pc2b.py [--dry-run|--apply] [--report FILE]`.
Matches on syntax (comments and strings masked, calls parsed with balanced brackets), never on
line numbers, and rewrites only exact mappings, so a re-run after a rebase is safe and a second
run changes nothing.  See README.md.
"""
import argparse
import bisect
import datetime
import json
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

# ---------------------------------------------------------------- mappings

# PhotoCraft `Tokens` field -> craft-libs colour path (themes.md Table 0), read from
# `craft_theme::Tokens` (`UiTokens` derefs to it).  Every entry is 1:1 by field.
COLOR_FIELDS = {
    "chrome": "color.surface.panel",
    "card": "color.surface.panel",
    "dock": "color.surface.background",
    "canvas": "color.surface.canvas",
    "field": "color.surface.sunken",
    "field_border": "color.line.control_edge",
    "card_border": "color.line.border",
    "separator": "color.line.divider",
    "hover": "color.state.hover",
    "pressed": "color.state.pressed",
    "row_selected": "color.state.selected",
    "text": "color.text.normal",
    "text_dim": "color.text.muted",
    "text_faint": "color.text.faint",
    "accent": "color.accent.solid",
    "accent_soft": "color.accent.soft",
    "accent_text": "color.accent.text",
    "danger": "color.status.bad.fg",
    "warning": "color.status.warn.fg",
    "histogram_bg": "color.data.scope",
    "caption_close": "color.caption.close_hover",
    "caption_close_text": "color.caption.close_text",
    "scrim": "color.surface.scrim",
    # component layer
    "primary_bg": "component.button.primary.fill.rest",
    "primary_text": "component.button.primary.text.rest",
    "tab_strip": "component.tab.strip",
}
# Fields with no single token (themes.md Table 0/2): left, with the reason.
AMBIGUOUS_FIELDS = {
    "icon": "no token: `text.normal` or an icon component",
    "accent_border": "no token: component or `line.focus`",
    "canvas_dot": "no token: needs `surface.canvas_pattern` or an app pin",
    "shadow": "elevation colour, not a colour token",
    "histogram_level": "u8 intensity, not a colour",
}
RADIUS_FIELDS = {"radius_sm": "control", "radius": "popup", "radius_lg": "window"}
RECEIVERS = ("t", "tokens", "theme", "tk")
ALL_FIELDS = sorted(set(COLOR_FIELDS) | set(AMBIGUOUS_FIELDS) | set(RADIUS_FIELDS), key=len, reverse=True)

SPACE = {0.0: "S0", 2.0: "S2", 4.0: "S4", 6.0: "S6", 8.0: "S8", 12.0: "S12", 16.0: "S16", 20.0: "S20", 24.0: "S24", 32.0: "S32"}
RADIUS = {0.0: "R0", 2.0: "R2", 3.0: "R3", 4.0: "R4", 6.0: "R6", 8.0: "R8", 12.0: "R12"}
STROKE = {1.0: "W1", 1.5: "W1_5", 2.0: "W2"}

PATTERNS = [
    ("token", "`t.<field>` token reads"),
    ("tokens_get", "`Tokens::get(ctx)`"),
    ("add_space", "`add_space(n)`"),
    ("stroke", "`Stroke::new(<width>, ..)`"),
    ("radius", "`rect_filled/rect_stroke` radius argument"),
    ("corner_radius", "`CornerRadius::same(n)` / `.corner_radius(n)` literals"),
]

# ---------------------------------------------------------------- lexing helpers

def mask(src: str) -> str:
    """Blank out comments and string/char literal contents; offsets and newlines are kept."""
    out = list(src)
    n = len(src)
    i = 0

    def blank(a, b):
        for k in range(a, b):
            if out[k] != "\n":
                out[k] = " "

    while i < n:
        c = src[i]
        if c == "/" and src.startswith("//", i):
            j = src.find("\n", i)
            j = n if j < 0 else j
            blank(i, j)
            i = j
        elif c == "/" and src.startswith("/*", i):
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
        elif c == "r" and re.match(r'r#*"', src[i:i + 8]) and (i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_")):
            m = re.match(r'r(#*)"', src[i:])
            end = '"' + m.group(1)
            j = src.find(end, i + m.end())
            j = n if j < 0 else j + len(end)
            blank(i + m.end(), j - len(end))
            i = j
        elif c == '"':
            j = i + 1
            while j < n and src[j] != '"':
                j += 2 if src[j] == "\\" else 1
            blank(i + 1, min(j, n))
            i = j + 1
        elif c == "'":
            m = re.match(r"'(\\.[^']*|[^\\'])'", src[i:i + 12])
            if m:
                blank(i + 1, i + m.end() - 1)
                i += m.end()
            else:
                i += 1
        else:
            i += 1
    return "".join(out)


def parse_args(m: str, open_idx: int):
    """Split the call whose '(' is at open_idx into top-level argument spans; returns (spans, close_idx)."""
    depth, i, start = 0, open_idx, open_idx + 1
    spans = []
    n = len(m)
    while i < n:
        c = m[i]
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
            if depth == 0:
                if m[start:i].strip():
                    spans.append((start, i))
                return spans, i
        elif c == "," and depth == 1:
            spans.append((start, i))
            start = i + 1
        i += 1
    return None, None


NUM = re.compile(r"^(\d+(?:\.\d*)?|\.\d+)(?:_?f32)?$")


def literal(text: str):
    mm = NUM.match(text.strip())
    return float(mm.group(1)) if mm else None


def fmt(v: float) -> str:
    return f"{v:g}"


def nearest(v: float, steps: dict, names: str):
    best = min(abs(v - s) for s in steps)
    near = [s for s in steps if abs(v - s) == best]
    label = "/".join(steps[s] for s in near)
    return f"nearest {names}{label} (diff {fmt(best)})"


# ---------------------------------------------------------------- file selection

def is_test_path(p: Path) -> bool:
    return p.name.endswith("_tests.rs") or p.name == "tests.rs" or "tests" in p.parts[:-1]


CFG_TEST_MOD_DECL = re.compile(r"#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;")
CFG_TEST_MOD_BLOCK = re.compile(r"#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{")


def collect_files(src: Path):
    files = sorted(p for p in src.rglob("*.rs"))
    gated = set()
    for p in files:
        for mm in CFG_TEST_MOD_DECL.finditer(mask(p.read_text())):
            base = p.parent if p.name in ("mod.rs", "lib.rs", "main.rs") else p.parent / p.stem
            gated.add(base / f"{mm.group(1)}.rs")
            gated.add(base / mm.group(1) / "mod.rs")
    keep = [p for p in files if not is_test_path(p.relative_to(src)) and p not in gated]
    return keep


def is_theme_file(rel: Path) -> bool:
    return rel.parts[0] == "theme.rs" or rel.parts[0] == "theme"


# ---------------------------------------------------------------- the scan

class Scan:
    def __init__(self):
        self.found = defaultdict(int)  # pattern -> sites
        self.theme_found = defaultdict(int)  # pattern -> sites inside theme.rs / theme/
        self.done = defaultdict(int)  # pattern -> already converted (not in found)
        self.rewritten = defaultdict(int)
        self.rewritten_detail = defaultdict(lambda: defaultdict(int))  # pattern -> label -> n
        self.left = defaultdict(lambda: defaultdict(list))  # pattern -> reason -> [(file, line)]
        self.edits = {}  # file -> [(start, end, text)]
        self.files = 0
        self.lines = 0


def receiver_evidence(m: str):
    """Names bound to a PhotoCraft `Tokens` value in this file."""
    names = set()
    for r in RECEIVERS:
        if re.search(rf"\b{r}\s*:\s*&?\s*(?:mut\s+)?(?:[\w]+::)*Tokens\b", m) or re.search(
            rf"\blet\s+(?:mut\s+)?{r}\b[^;=]*=\s*&?\s*(?:[\w]+::)*Tokens::(?:get|for_kind|of)\b", m
        ) or re.search(rf"\b{r}\s*=\s*&?\s*(?:[\w]+::)*Tokens::(?:get|for_kind|of)\b", m):
            names.add(r)
    return names


def scan_file(path: Path, rel: Path, scan: Scan, scale: str):
    text = path.read_text()
    m = mask(text)
    cut = CFG_TEST_MOD_BLOCK.search(m)
    end_of_code = cut.start() if cut else len(m)
    m_code = m[:end_of_code]
    nl = [i for i, c in enumerate(text) if c == "\n"]
    scan.files += 1
    scan.lines += text[:end_of_code].count("\n") + 1
    theme = is_theme_file(rel)
    edits = []

    def line(i):
        return bisect.bisect_left(nl, i) + 1

    def record_found(pat):
        (scan.theme_found if theme else scan.found)[pat] += 1

    def leave(pat, reason, i):
        if not theme:
            scan.left[pat][reason].append((str(rel), line(i)))

    def rewrite(pat, a, b, new, label):
        if theme:
            return
        edits.append((a, b, new))
        scan.rewritten[pat] += 1
        scan.rewritten_detail[pat][label] += 1

    # ---- 1. token reads
    evidence = receiver_evidence(m_code)
    rx = re.compile(r"(?<![\w])(\.?\s*)\b(" + "|".join(RECEIVERS) + r")\.(" + "|".join(ALL_FIELDS) + r")\b(?!\s*\()")
    for mm in rx.finditer(m_code):
        # a field path (`self.theme.card`) has a '.' directly before the receiver
        pre = m_code[mm.start(): mm.start(2)]
        recv, field = mm.group(2), mm.group(3)
        a, b = mm.start(2), mm.end(3)
        if "." in pre:
            record_found("token")
            leave("token", "receiver reached through a field path (`x.theme.<field>`)", a)
            continue
        if field in RADIUS_FIELDS and re.match(r"\.(?:control|popup|window)\b", m_code[b:b + 10]):
            if not theme:
                scan.done["token"] += 1
            continue
        record_found("token")
        tail = m_code[b:b + 6]
        if re.match(r"\s*=(?!=)", tail):
            leave("token", "assignment to the field", a)
            continue
        if field in AMBIGUOUS_FIELDS:
            leave("token", f"`{field}`: {AMBIGUOUS_FIELDS[field]}", a)
            continue
        if recv not in evidence:
            leave("token", f"receiver `{recv}` not bound to `Tokens` in this file", a)
            continue
        if field in RADIUS_FIELDS:
            # f32 -> CornerRadius changes the type, so only plain call arguments are exact
            before = m_code[:a].rstrip()[-1:]
            after = m_code[b:].lstrip()[:1]
            if before in "(," and after in "),":
                rewrite("token", a, b, f"{recv}.radius.{RADIUS_FIELDS[field]}", f"{field} -> radius.{RADIUS_FIELDS[field]}")
            else:
                leave("token", f"`{field}` used as f32 (arithmetic, cast or method), not a plain argument", a)
            continue
        tok = COLOR_FIELDS[field]
        rewrite("token", a, b, f"{recv}.{tok}.to_egui()", f"{field} -> {tok}")

    # ---- 2. Tokens::get
    for mm in re.finditer(r"(?<![\w])((?:\w+::)*)Tokens::get\s*\(", m_code):
        record_found("tokens_get")
        spans, close = parse_args(m_code, mm.end() - 1)
        if spans is None or len(spans) != 1:
            leave("tokens_get", "arguments not parsed", mm.start())
            continue
        arg = m_code[spans[0][0]:spans[0][1]].strip()
        am = re.fullmatch(r"&?\s*(\w+)\.ctx\(\)", arg)
        if am and re.fullmatch(r"(?:\w*_)?ui(?:_\w+)?|ui\d*", am.group(1)):
            rewrite("tokens_get", mm.start(), close + 1, f"{mm.group(1)}Tokens::of({am.group(1)})", "Tokens::get(ui.ctx()) -> Tokens::of(ui)")
        else:
            leave("tokens_get", f"argument `{arg[:30]}` is not `<ui>.ctx()`", mm.start())

    # ---- 3. add_space
    for mm in re.finditer(r"\badd_space\s*\(", m_code):
        spans, close = parse_args(m_code, mm.end() - 1)
        if spans is None or len(spans) != 1:
            continue
        a, b = spans[0]
        arg = m_code[a:b].strip()
        if f"{scale}::Space::" in arg:
            if not theme:
                scan.done["add_space"] += 1
            continue
        record_found("add_space")
        v = literal(arg)
        if v is None:
            neg = re.fullmatch(r"-\s*(\d+(?:\.\d*)?)(?:_?f32)?", arg)
            leave("add_space", "negative literal" if neg else "not a literal (expression or variable)", mm.start())
        elif v in SPACE:
            ws_a = a + (len(m_code[a:b]) - len(m_code[a:b].lstrip()))
            rewrite("add_space", ws_a, ws_a + len(arg), f"{scale}::Space::{SPACE[v]}.px()", f"{fmt(v)} -> Space::{SPACE[v]}")
        else:
            leave("add_space", f"off-scale {fmt(v)}: " + nearest(v, SPACE, ""), mm.start())

    # ---- 4. Stroke::new
    for mm in re.finditer(r"(?<![\w])((?:\w+::)*)Stroke::new\s*\(", m_code):
        spans, close = parse_args(m_code, mm.end() - 1)
        if not spans:
            continue
        a, b = spans[0]
        arg = m_code[a:b].strip()
        if f"{scale}::Stroke::" in arg:
            if not theme:
                scan.done["stroke"] += 1
            continue
        record_found("stroke")
        v = literal(arg)
        if v is None:
            leave("stroke", "width not a literal", mm.start())
        elif v in STROKE:
            ws_a = a + (len(m_code[a:b]) - len(m_code[a:b].lstrip()))
            rewrite("stroke", ws_a, ws_a + len(arg), f"{scale}::Stroke::{STROKE[v]}.width(1.0)", f"{fmt(v)} -> Stroke::{STROKE[v]}")
        elif v == 0.0:
            leave("stroke", "zero width (no step; `Stroke::NONE`)", mm.start())
        else:
            leave("stroke", f"off-scale {fmt(v)}: " + nearest(v, {1.0: "W1", 1.5: "W1_5", 2.0: "W2"}, "") + "; Hairline is 1/ppp", mm.start())

    # ---- 5. rect_filled / rect_stroke radius
    for mm in re.finditer(r"\.(rect_filled|rect_stroke)\s*\(", m_code):
        spans, close = parse_args(m_code, mm.end() - 1)
        if not spans or len(spans) < 3:
            continue
        a, b = spans[1]
        arg = m_code[a:b].strip()
        if f"{scale}::Radius::" in arg:
            if not theme:
                scan.done["radius"] += 1
            continue
        record_found("radius")
        v = literal(arg)
        if v is None:
            kind = "token read, see the token rows" if re.match(r"(?:" + "|".join(RECEIVERS) + r")\.radius", arg) else "variable or expression"
            leave("radius", f"not a literal ({kind})", mm.start())
        elif v in RADIUS:
            ws_a = a + (len(m_code[a:b]) - len(m_code[a:b].lstrip()))
            rewrite("radius", ws_a, ws_a + len(arg), f"{scale}::Radius::{RADIUS[v]}.px()", f"{fmt(v)} -> Radius::{RADIUS[v]}")
        else:
            leave("radius", f"off-scale {fmt(v)}: " + nearest(v, RADIUS, ""), mm.start())

    # ---- 6. CornerRadius literals (u8 API: counted, left)
    for mm in re.finditer(r"(?:\bCornerRadius::same\s*\(|\.corner_radius\s*\()\s*(\d+(?:\.\d*)?)", m_code):
        record_found("corner_radius")
        leave("corner_radius", "u8 `CornerRadius` API; convert with `corner_radius(Radius::Rn.px())` by hand", mm.start())

    if edits:
        scan.edits[path] = edits
    return text


IMPORT = "use craft_ui::theme::ToEgui;"


def add_import(text: str) -> str:
    if re.search(r"\bToEgui\b", text):
        return text
    m = mask(text)
    mm = re.search(r"^use\s", m, re.M)
    if not mm:
        return IMPORT + "\n" + text
    depth, i = 0, mm.start()
    while i < len(m):
        if m[i] == "{":
            depth += 1
        elif m[i] == "}":
            depth -= 1
        elif m[i] == ";" and depth == 0:
            break
        i += 1
    return text[: i + 1] + "\n" + IMPORT + text[i + 1:]


def apply_edits(text: str, edits):
    for a, b, new in sorted(edits, reverse=True):
        text = text[:a] + new + text[b:]
    return text


# ---------------------------------------------------------------- report

def md_report(scan: Scan, commit: str, args) -> str:
    today = datetime.date.today().isoformat()
    out = [
        "# PC-2b dry-run report: token reads and size/colour literals",
        "",
        f"- Run date: {today}",
        f"- Source: PhotoCraft `{commit}`, `{args.src}` (script `scripts/codemods/pc2b/pc2b.py`, mode `{'apply' if args.apply else 'dry-run'}`)",
        f"- Scope: {scan.files} non-test `.rs` files, {scan.lines:,} lines (test files, `#[cfg(test)]`-gated modules and inline test blocks cut). `theme.rs` is scanned but never rewritten; its hits are in the last column.",
        f"- Target: craft-libs `design-system` @ `e13f09f2`. Scale paths are written as `{args.scale}::Space::S4.px()` etc.",
        "",
        "## Totals",
        "",
        "| Pattern | Sites found | Rewritten | Left for hand work | Already converted | In theme.rs (not rewritten) |",
        "|---|---|---|---|---|---|",
    ]
    for key, label in PATTERNS:
        left = sum(len(v) for v in scan.left[key].values())
        out.append(f"| {label} | {scan.found[key]} | {scan.rewritten[key]} | {left} | {scan.done[key]} | {scan.theme_found[key]} |")
    out.append("")
    for key, label in PATTERNS:
        out += [f"## {label}", ""]
        if scan.rewritten_detail[key]:
            out += ["Rewritten:", "", "| Mapping | Sites |", "|---|---|"]
            for lab, n in sorted(scan.rewritten_detail[key].items(), key=lambda kv: -kv[1]):
                out.append(f"| {lab} | {n} |")
            out.append("")
        if scan.left[key]:
            out += ["Left for hand work:", "", "| Reason | Sites | Examples |", "|---|---|---|"]
            for reason, sites in sorted(scan.left[key].items(), key=lambda kv: -len(kv[1])):
                ex = ", ".join(f"`{f}:{l}`" for f, l in sites[:3])
                out.append(f"| {reason.replace('|', '/')} | {len(sites)} | {ex} |")
            out.append("")
    out += ["## Off-scale values (by frequency)", "", "| Pattern | Value: sites |", "|---|---|"]
    for key in ("add_space", "stroke", "radius"):
        counts = defaultdict(int)
        for reason, sites in scan.left[key].items():
            mo = re.match(r"off-scale ([\d.]+)", reason)
            if mo:
                counts[mo.group(1)] += len(sites)
        top = ", ".join(f"{v}: {n}" for v, n in sorted(counts.items(), key=lambda kv: -kv[1]))
        out.append(f"| {dict(PATTERNS)[key]} | {top} |")
    out += ["", MISSING, ""]
    return "\n".join(out)


MISSING = """## Missing in craft-libs

Checked against `design-system` @ `e13f09f2` (`crates/theme/src/{scale,tokens}.rs`, `crates/ui/src/theme`).

| Needed | Why | Sites |
|---|---|---|
| `Space`, `Radius`, `Stroke` re-exported from `craft_ui` (or a `craft-theme` dependency in `photocraft-ui-egui`) | PhotoCraft depends on `craft-ui` only; the emitted `craft_theme::Space::S4.px()` does not resolve today. `--scale craft_ui::theme` switches the prefix once the re-export exists. | every scale rewrite |
| A token for `icon` (glyph colour that is not `text.normal`) | `t.icon` has no one-to-one role | see token table |
| A token for `accent_border` (or an agreed use of `line.focus`) | no role | see token table |
| `surface.canvas_pattern` (or an app pin) for `canvas_dot` | no role | see token table |
| A colour token or component for `shadow` | `Tokens.shadow` is a colour; the DS has elevation geometry only (`Shadows`) | see token table |
| `histogram_level` (u8 intensity) | derivable, no token | see token table |
| `f32` radius accessor on `UiTokens` | `UiTokens.radius.*` is `CornerRadius` (u8); `tokens.shape.radius.*` is f32 and needs the deref path. Sites that do arithmetic on `t.radius*` are left. | see token table |
| `Space` steps 3, 10, 14, 22 (or an agreed rounding) | frequent off-scale literals, see the off-scale table | see off-scale table |
| `Radius` step 1 | `1.0` radius is common, nearest steps are R0/R2 | see off-scale table |
| `Stroke` steps for 0.75, 1.25, 3.0, or agreed rounding | the scale has only Hairline/W1/W1_5/W2 | see off-scale table |
| `Tokens::of` taking a `Context` | sites with only a `ctx` (or `painter.ctx()`) are left; `craft_ui::theme::current(ctx)` exists and returns the same value | `Tokens::get` table |

## Notes

- `chrome` and `card` both map to `surface.panel`; they differ in the Studio family (themes.md Table 0). Pin the difference in the theme files.
- Radius literals map to the scale step with the same value (`Radius::R4.px()`), not to the roles `radius.control/popup/window` that codemods.md Table 4 suggests: a role changes the value per theme, so it is not an exact mapping.
- A file that rewrites `Tokens::get` but keeps a ctx-only call, or rewrites reads on one binding but not another, will not compile until the leftovers are done by hand: after a rewrite `t` is a `UiTokens`, and `t.color...` exists only on it.
- Receiver check is per file: `t`, `tokens`, `theme`, `tk` is rewritten only when the file binds that name to a `Tokens` (`let t = ..Tokens::get(..)`, `t: &Tokens`). A second, unrelated `t` in the same file would be rewritten too; review the diff.
- Rewritten colour reads add `use craft_ui::theme::ToEgui;` after the file's first `use` statement.
"""


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    g = ap.add_mutually_exclusive_group()
    g.add_argument("--dry-run", action="store_true", help="count and report; write nothing to source (default)")
    g.add_argument("--apply", action="store_true", help="rewrite exact matches in place")
    ap.add_argument("--root", default=None, help="PhotoCraft checkout (default: the checkout containing this script)")
    ap.add_argument("--src", default="crates/ui-egui/src")
    ap.add_argument("--scale", default="craft_theme", help="module path of Space/Radius/Stroke in emitted code")
    ap.add_argument("--report", help="write the markdown report here")
    ap.add_argument("--json", help="write raw counts here")
    ap.add_argument("--dump-left", help="write every left site (pattern, reason, file:line) to this file")
    args = ap.parse_args()

    root = Path(args.root) if args.root else Path(__file__).resolve().parents[3]
    src = root / args.src
    if not src.is_dir():
        sys.exit(f"no such directory: {src}")
    commit = subprocess.run(["git", "-C", str(root), "rev-parse", "--short=10", "HEAD"], capture_output=True, text=True).stdout.strip()

    scan = Scan()
    texts = {}
    for p in collect_files(src):
        texts[p] = scan_file(p, p.relative_to(src), scan, args.scale)

    if args.apply:
        for p, edits in scan.edits.items():
            new = apply_edits(texts[p], edits)
            if any("to_egui()" in e[2] for e in edits):
                new = add_import(new)
            p.write_text(new)

    if args.report:
        Path(args.report).write_text(md_report(scan, commit, args) + "\n")
    if args.json:
        Path(args.json).write_text(json.dumps({"found": scan.found, "rewritten": scan.rewritten, "files": scan.files}, indent=1))
    if args.dump_left:
        with open(args.dump_left, "w") as f:
            for pat, d in scan.left.items():
                for reason, sites in d.items():
                    for file, ln in sites:
                        f.write(f"{pat}\t{file}:{ln}\t{reason}\n")
    for key, label in PATTERNS:
        left = sum(len(v) for v in scan.left[key].values())
        print(f"{label:55} found {scan.found[key]:5}  rewritten {scan.rewritten[key]:5}  left {left:5}  done {scan.done[key]:4}  theme.rs {scan.theme_found[key]}")


if __name__ == "__main__":
    main()
