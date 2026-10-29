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
    # DS-43 B2: the port normalises glyph colour to `text.normal`
    "icon": "color.text.normal",
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
    "accent_border": "DS-43 B3: per use (selected look, `ToggleButton`/`Segmented`, or `accent.solid` stroke)",
    "canvas_dot": "DS-43 B4: app-side (canvas colour moved 0.06 in OKLCH lightness)",
    "shadow": "DS-43 B5: two uses (floating-panel shadow -> `shadow.popup/dialog`; cursor halo -> `overlay.halo`)",
    "histogram_level": "DS-43 B6: dropped; `histogram_color`/`scope_hue` move to `data.*`",
}
RADIUS_FIELDS = {"radius_sm": "control", "radius": "popup", "radius_lg": "window"}
RECEIVERS = ("t", "tokens", "theme", "tk")
ALL_FIELDS = sorted(set(COLOR_FIELDS) | set(AMBIGUOUS_FIELDS) | set(RADIUS_FIELDS), key=len, reverse=True)

SPACE = {0.0: "S0", 2.0: "S2", 4.0: "S4", 6.0: "S6", 8.0: "S8", 12.0: "S12", 16.0: "S16", 20.0: "S20", 24.0: "S24", 32.0: "S32"}
RADIUS = {0.0: "R0", 2.0: "R2", 3.0: "R3", 4.0: "R4", 6.0: "R6", 8.0: "R8", 12.0: "R12"}
STROKE = {1.0: "W1", 1.5: "W1_5", 2.0: "W2"}
# DS-43 B8/B9: decided roundings (no new steps). Counted apart from exact rewrites; --exact-only turns them off.
SPACE_ROUND = {3.0: "S4", 5.0: "S4", 7.0: "S8", 9.0: "S8", 10.0: "S12", 14.0: "S12", 18.0: "S16", 22.0: "S24", 26.0: "S24", 46.0: "S32"}
RADIUS_ROUND = {1.0: "R2"}
# DS-43 B10: where each off-scale stroke width goes (a hint for the hand pass, never rewritten)
STROKE_HINT = {
    1.15: "glyph or box outline: Icon/Disclosure or W1", 1.2: "glyph or box outline: Icon/Disclosure or W1", 1.25: "glyph or box outline: Icon/Disclosure or W1",
    1.3: "glyph or box outline: Icon/Disclosure or W1", 0.75: "brush ring: app canvas ink", 0.7: "scope line: histogram.line or app canvas ink",
    0.6: "scope line: histogram.line or app canvas ink", 0.5: "app canvas ink", 0.3: "app canvas ink",
    2.5: "halo: component token or app canvas ink", 3.0: "halo: component token or app canvas ink", 3.5: "halo: component token or app canvas ink",
    4.5: "halo: component token or app canvas ink", 5.0: "tool-button arc: DS-35 Toolbar",
}

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
        self.rounded = defaultdict(int)  # pattern -> rewrites that apply a decided rounding (subset of rewritten)
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


ROUND = True


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

    fns = []
    for fm in re.finditer(r"\bfn\s+\w+\s*(?:<[^>{}]*>)?\s*\(", m_code):
        spans, close = parse_args(m_code, fm.end() - 1)
        if spans is not None:
            fns.append((fm.start(), m_code[fm.end() - 1:close + 1]))
    fn_starts = [f[0] for f in fns]

    def ctx_expr(i):
        """An expression for the egui Context at offset i, from the enclosing fn's parameters and closures; None if unknown."""
        k = bisect.bisect_right(fn_starts, i) - 1
        if k < 0:
            return None
        params = fns[k][1]
        body = m_code[fn_starts[k]:i]
        if re.search(r"\bui\s*:", params) or re.search(r"\|\s*(?:mut\s+)?ui\s*[,|:]", body):
            return "ui.ctx()"
        if re.search(r"\bctx\s*:\s*&", params):
            return "ctx"
        if re.search(r"\bctx\s*:", params) or re.search(r"\blet\s+ctx\b", body):
            return "&ctx"
        if re.search(r"\bpainter\s*:", params) or re.search(r"\blet\s+(?:mut\s+)?painter\b", body):
            return "painter.ctx()"
        return None

    def record_found(pat):
        (scan.theme_found if theme else scan.found)[pat] += 1

    def leave(pat, reason, i):
        if not theme:
            scan.left[pat][reason].append((str(rel), line(i)))

    def rewrite(pat, a, b, new, label, rounded=False):
        if theme:
            return
        edits.append((a, b, new))
        scan.rewritten[pat] += 1
        if rounded:
            scan.rounded[pat] += 1
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
            # DS-43 B7: the f32 value lives on the `Tokens` deref path; `t.radius.*` stays `CornerRadius`
            role = RADIUS_FIELDS[field]
            rewrite("token", a, b, f"{recv}.shape.radius.{role}", f"{field} -> shape.radius.{role}")
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
            continue
        # DS-43 B11: any other Context goes through `craft_ui::theme::current(&Context)`
        if re.fullmatch(r"&?\s*(?:self\.)?\w+\.ctx\(\)", arg):
            ctx = arg.lstrip("& ")
        elif re.fullmatch(r"&\s*[\w.]+", arg):
            ctx = arg
        elif re.fullmatch(r"\w+", arg):
            k = bisect.bisect_right(fn_starts, mm.start()) - 1
            is_ref = k >= 0 and re.search(rf"\b{arg}\s*:\s*&", fns[k][1])
            ctx = arg if is_ref else "&" + arg
        else:
            leave("tokens_get", f"argument `{arg[:30]}` is not a Context expression", mm.start())
            continue
        rewrite("tokens_get", mm.start(), close + 1, f"craft_ui::theme::current({ctx})", "Tokens::get(<ctx>) -> craft_ui::theme::current(<ctx>)")

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
        ws_a = a + (len(m_code[a:b]) - len(m_code[a:b].lstrip()))
        if v is None:
            neg = re.fullmatch(r"-\s*(\d+(?:\.\d*)?)(?:_?f32)?", arg)
            leave("add_space", "negative literal" if neg else "not a literal (expression or variable)", mm.start())
        elif v in SPACE:
            rewrite("add_space", ws_a, ws_a + len(arg), f"{scale}::Space::{SPACE[v]}.px()", f"{fmt(v)} -> Space::{SPACE[v]}")
        elif v in SPACE_ROUND and ROUND:
            st = SPACE_ROUND[v]
            rewrite("add_space", ws_a, ws_a + len(arg), f"{scale}::Space::{st}.px()", f"{fmt(v)} -> Space::{st} (rounded, DS-43 B8)", rounded=True)
        else:
            leave("add_space", f"off-scale {fmt(v)}: " + nearest(v, SPACE, ""), mm.start())

    # ---- 4. Stroke::new
    for mm in re.finditer(r"(?<![\w])((?:\w+::)*)Stroke::new\s*\(", m_code):
        spans, close = parse_args(m_code, mm.end() - 1)
        if not spans:
            continue
        a, b = spans[0]
        arg = m_code[a:b].strip()
        record_found("stroke")
        v = literal(arg)
        if v is None:
            leave("stroke", "width not a literal", mm.start())
        elif v in STROKE:
            if len(spans) != 2:
                leave("stroke", "colour argument not parsed", mm.start())
                continue
            ctx = ctx_expr(mm.start())
            if ctx is None:
                leave("stroke", "no `ui`, `ctx` or `painter` in the enclosing fn to take pixels_per_point from", mm.start())
                continue
            # replace `Stroke::new(<width>` only; `, <colour>)` stays so edits inside the colour expression still apply
            wa = a + (len(m_code[a:b]) - len(m_code[a:b].lstrip()))
            rewrite("stroke", mm.start(), wa + len(arg), f"{scale}::Stroke::{STROKE[v]}.egui({ctx}", f"{fmt(v)} -> Stroke::{STROKE[v]}.egui(ctx, colour)")
        elif v == 0.0:
            leave("stroke", "zero width (no step; `Stroke::NONE`)", mm.start())
        else:
            hint = STROKE_HINT.get(v, "no B10 row")
            leave("stroke", f"off-scale {fmt(v)} ({hint}): " + nearest(v, STROKE, ""), mm.start())
    if not theme:
        scan.done["stroke"] += len(re.findall(re.escape(scale) + r"::Stroke::\w+\.egui\(", m_code))

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
        ws_a = a + (len(m_code[a:b]) - len(m_code[a:b].lstrip()))
        if v is None:
            kind = "token read, see the token rows" if re.match(r"(?:" + "|".join(RECEIVERS) + r")\.(?:shape\.)?radius", arg) else "variable or expression"
            leave("radius", f"not a literal ({kind})", mm.start())
        elif v in RADIUS:
            rewrite("radius", ws_a, ws_a + len(arg), f"{scale}::Radius::{RADIUS[v]}.px()", f"{fmt(v)} -> Radius::{RADIUS[v]}")
        elif v in RADIUS_ROUND and ROUND:
            st = RADIUS_ROUND[v]
            rewrite("radius", ws_a, ws_a + len(arg), f"{scale}::Radius::{st}.px()", f"{fmt(v)} -> Radius::{st} (rounded, DS-43 B9)", rounded=True)
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
        f"- Target: craft-libs `design-system` @ `01e20c71` (API as built in DS-43). Scale paths are written as `{args.scale}::Space::S4.px()` etc.",
        "",
        "## Totals",
        "",
        "| Pattern | Sites found | Rewritten | of which rounded (DS-43 B8/B9) | Left for hand work | Already converted | In theme.rs (not rewritten) |",
        "|---|---|---|---|---|---|---|",
    ]
    for key, label in PATTERNS:
        left = sum(len(v) for v in scan.left[key].values())
        out.append(f"| {label} | {scan.found[key]} | {scan.rewritten[key]} | {scan.rounded[key]} | {left} | {scan.done[key]} | {scan.theme_found[key]} |")
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
        top = ", ".join(f"{v}: {n}" for v, n in sorted(counts.items(), key=lambda kv: -kv[1])) or "none"
        out.append(f"| {dict(PATTERNS)[key]} | {top} |")
    out += ["", MISSING, ""]
    return "\n".join(out)


MISSING = """## Missing in craft-libs

Checked against `design-system` @ `01e20c71`. The scale re-export (`craft_ui::theme::scale`), `StrokeExt::egui`, `craft_ui::theme::current(&Context)`, `ToEgui` and the f32 radii (`t.shape.radius.*` through the `UiTokens` deref) all exist, and every token path the script emits was found in `crates/theme/src/tokens.rs` and `components/`. The removed tokens `menu.tint_alpha` and `histogram.fill_alpha` are not emitted. Nothing in craft-libs blocks a rewrite. What is left is decided in DS-43 as port or app work, so it is not a craft-libs gap:

| Left | Decision (DS-43) | Where |
|---|---|---|
| `shadow` reads | B5: floating-panel shadow becomes `UiTokens.shadow.popup/dialog` by size, a cursor halo becomes `overlay.halo`; chosen per use | token table |
| `accent_border` reads | B3: per use (selected look, `ToggleButton`/`Segmented`, or an `accent.solid` stroke) | token table |
| `canvas_dot` reads | B4: app-side, derived from the canvas colour in use | token table |
| `histogram_level` | B6: dropped with `histogram_color`/`scope_hue` moving to `data.*` | token table |
| Off-scale `Stroke` widths | B10: no new steps; glyphs become `Icon`/`Disclosure`, halos component tokens (`handle.halo`, `gradient_line.halo`, `marquee.drawing_halo`) or app canvas ink | stroke table, hints in the reasons |
| Space/Radius values not listed in B8/B9 (e.g. Space 46, Radius 0.5, 1.5, 8.5) | no decision | off-scale table |
| `Stroke::new` in a fn with no `ui`, `ctx` or `painter` | the codemod cannot name a `Context`; thread one through by hand | stroke table |

## Notes

- `chrome` and `card` both map to `surface.panel`; they differ in the Studio family (themes.md Table 0). Pin the difference in the theme files.
- `icon` maps to `text.normal` and `radius_sm/radius/radius_lg` to `t.shape.radius.control/popup/window` (f32), as DS-43 B2 and B7 decide. A rewritten `t.radius_sm - 1.0` keeps its f32 type.
- Radius literals map to the scale step with the same value (`Radius::R4.px()`), not to the roles of codemods.md Table 4: a role changes the value per theme.
- Rounded rewrites (DS-43 B8/B9) change the pixel value: Space 3 and 5 to S4, 7 and 9 to S8, 10 and 14 to S12, 18 to S16, 22 and 26 to S24, 46 to S32; Radius 1 to R2. `--exact-only` turns them off. Where a site separates rows or groups, `gap.stack`/`gap.section` is the better target; that is a hand pass.
- `Stroke::new(<w>, <colour>)` becomes `scale::Stroke::W1.egui(<ctx>, <colour>)`; `<ctx>` is `ui.ctx()`, `ctx`/`&ctx` or `painter.ctx()`, taken from the enclosing fn's parameters.
- `Tokens::get(<ui>.ctx())` becomes `Tokens::of(<ui>)`; any other Context becomes `craft_ui::theme::current(<ctx>)`. After a rewrite `t` is a `UiTokens`, so every read in the file must be rewritten too; a file with a left read will not compile until it is done by hand.
- Receiver check is per file: `t`, `tokens`, `theme`, `tk` is rewritten only when the file binds that name to a `Tokens`. A second, unrelated `t` in the same file would be rewritten too; review the diff.
- Rewritten colour reads add `use craft_ui::theme::ToEgui;` after the file's first `use` statement.
"""


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    g = ap.add_mutually_exclusive_group()
    g.add_argument("--dry-run", action="store_true", help="count and report; write nothing to source (default)")
    g.add_argument("--apply", action="store_true", help="rewrite exact matches in place")
    ap.add_argument("--root", default=None, help="PhotoCraft checkout (default: the checkout containing this script)")
    ap.add_argument("--src", default="crates/ui-egui/src")
    ap.add_argument("--scale", default="craft_ui::theme::scale", help="module path of Space/Radius/Stroke in emitted code")
    ap.add_argument("--exact-only", action="store_true", help="skip the DS-43 B8/B9 roundings (Space 3, 10, 14, ..; Radius 1)")
    ap.add_argument("--report", help="write the markdown report here")
    ap.add_argument("--source-commit", help="commit named in the report (default: HEAD of the checkout)")
    ap.add_argument("--json", help="write raw counts here")
    ap.add_argument("--dump-left", help="write every left site (pattern, reason, file:line) to this file")
    args = ap.parse_args()

    global ROUND
    ROUND = not args.exact_only
    root = Path(args.root) if args.root else Path(__file__).resolve().parents[3]
    src = root / args.src
    if not src.is_dir():
        sys.exit(f"no such directory: {src}")
    commit = subprocess.run(["git", "-C", str(root), "rev-parse", "--short=10", "HEAD"], capture_output=True, text=True).stdout.strip()
    commit = args.source_commit or commit

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
