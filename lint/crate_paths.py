#!/usr/bin/env python3
"""Keep references short: reject/fix `crate::path` in code, leaving `use` lines alone.

Usage:
    python lint/crate_paths.py check [--root .]
    python lint/crate_paths.py fix   [--root .]

The check is meant for CI. The fix is deliberately conservative: it imports the first
segment (`crate::module` -> `module`) once per lexical module scope, skips names that
already exist in that file, and never edits a `use` declaration. Run cargo fmt/check after it.
"""

from __future__ import annotations
import argparse, pathlib, re, subprocess, sys

SKIP_PARTS = {"target", ".git", ".cppls"}
USE = re.compile(r"^\s*(?:pub(?:\s*\([^)]*\))?\s+)?use\b")
MOD = re.compile(r"\b(?:pub(?:\s*\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{")
PATH = re.compile(r"\bcrate::([A-Za-z_][A-Za-z0-9_]*)")
MACRO = re.compile(r"^\s*(?:macro_rules!|macro)\s+([A-Za-z_][A-Za-z0-9_]*)")


def blank(seg: str) -> str:
    return "".join("\n" if c == "\n" else " " for c in seg)


def masked(text: str) -> str:
    out = []
    i = 0
    n = len(text)
    while i < n:
        if text.startswith("//", i):
            j = text.find("\n", i)
            if j < 0:
                j = n
            out.append(blank(text[i:j]))
            i = j
            continue
        if text.startswith("/*", i):
            depth = 1
            j = i + 2
            while j < n and depth:
                if text.startswith("/*", j):
                    depth += 1
                    j += 2
                elif text.startswith("*/", j):
                    depth -= 1
                    j += 2
                else:
                    j += 1
            out.append(blank(text[i:j]))
            i = j
            continue
        # raw string: r"..." / r#"..."# / br#"..."#
        m = re.match(r'b?r(#*)"', text[i:])
        if m:
            hashes = m.group(1)
            start = i
            i += m.end()
            close = '"' + hashes
            j = text.find(close, i)
            if j < 0:
                j = n
            else:
                j += len(close)
            out.append(blank(text[start:j]))
            i = j
            continue
        if text[i] in ('"', "'"):
            quote = text[i]
            # A lone apostrophe is a lifetime, not a char literal.
            if quote == "'" and not (i + 2 < n and text[i + 2] == "'"):
                out.append("'")
                i += 1
                continue
            j = i + 1
            while j < n:
                if text[j] == "\\":
                    j += 2
                    continue
                if text[j] == quote:
                    j += 1
                    break
                j += 1
            out.append(blank(text[i:j]))
            i = j
            continue
        out.append(text[i])
        i += 1
    return "".join(out)


def scopes(m: str):
    """(start_line, end_line, indent) for each inline module, 0 = file."""
    stack = [(0, len(m.splitlines()), 0)]
    depth = 0
    cur = []
    lines = m.split("\n")
    for no, line in enumerate(lines, 1):
        mm = MOD.search(line)
        if mm:
            indent = len(line) - len(line.lstrip())
            stack.append((no, len(lines), indent))
            continue
        depth += line.count("{") - line.count("}")
    # approximate end by matching from module start
    result = [(0, len(lines), 0)]
    for idx in range(1, len(stack)):
        start, _, indent = stack[idx]
        d = 0
        end = len(lines)
        for no in range(start, len(lines) + 1):
            d += lines[no - 1].count("{") - lines[no - 1].count("}")
            if d == 0:
                end = no
                break
        result.append((start, end, indent))
    return result


def macro_spans(m: str):
    lines = m.split("\n")
    spans = []
    for no, line in enumerate(lines, 1):
        if not MACRO.match(line):
            continue
        d = 0
        end = no
        for at in range(no, len(lines) + 1):
            d += lines[at - 1].count("{") - lines[at - 1].count("}")
            if "{" in "\n".join(lines[no - 1 : at]) and d <= 0:
                end = at
                break
        spans.append((no, end))
    return spans


def file_info(path: pathlib.Path):
    text = path.read_text(encoding="utf-8")
    m = masked(text)
    lines = text.split("\n")
    ml = m.split("\n")
    scopes_list = scopes(m)

    def owner(no):
        best = scopes_list[0]
        for s in scopes_list:
            if s[0] <= no <= s[1] and s[0] >= best[0]:
                best = s
        return best

    occurrences = []
    macros = macro_spans(m)
    in_use = False
    for no, line in enumerate(ml, 1):
        if any(a <= no <= b for a, b in macros):
            continue
        s = line.strip()
        if in_use or USE.match(line):
            if ";" in line:
                in_use = False
            elif USE.match(line):
                in_use = True
            continue
        for mm in PATH.finditer(line):
            occurrences.append((no, mm.start(), mm.group(1), owner(no)))
    return text, lines, occurrences


def _split_top(text: str):
    parts = []
    depth = 0
    start = 0
    for i, c in enumerate(text):
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
        elif c == "," and depth == 0:
            parts.append(text[start:i])
            start = i + 1
    parts.append(text[start:])
    return parts


def _use_names(tree: str, out: set):
    tree = tree.strip()
    if not tree:
        return
    if tree.startswith("{") and tree.endswith("}"):
        for part in _split_top(tree[1:-1]):
            _use_names(part, out)
        return
    if "{" in tree:
        prefix, rest = tree.split("{", 1)
        contents = rest.rsplit("}", 1)[0]
        prefix = prefix.rstrip(":")
        for part in _split_top(contents):
            part = part.strip()
            if part == "self":
                name = prefix.split("::")[-1].strip()
                if name:
                    out.add(name)
            else:
                _use_names(prefix + "::" + part, out)
        return
    tree = tree.strip().strip(":").strip()
    if not tree or tree in ("*", "crate", "super", "self", "std", "core", "alloc"):
        return
    if " as " in tree:
        tree = tree.split(" as ")[-1].strip()
    name = tree.split("::")[-1].strip()
    if name and re.match(r"^[A-Za-z_][A-Za-z0-9_]*$", name):
        out.add(name)


def existing_names(text):
    names = set()
    # Module declarations are not imports: a `mod config` in a sibling or
    # nested scope does not make `config::...` visible here.
    joined = " ".join(masked(text).splitlines())
    for m in re.finditer(r"\buse\s+([^;]+);", joined):
        _use_names(m.group(1), names)
    return names


def check(paths):
    total = 0
    files = 0
    for p in paths:
        text, lines, occ = file_info(p)
        if occ:
            files += 1
            total += len(occ)
            for no, _c, seg, sc in occ[:5]:
                print(f"{p}:{no}: crate::{seg} (scope starts line {sc[0]})")
            if len(occ) > 5:
                print(f"{p}: ... {len(occ)-5} more")
    print(f"crate paths: {total} occurrence(s) in {files} file(s)")
    return 1 if total else 0


def fix(paths, verify=True):
    backups = {p: p.read_text(encoding="utf-8") for p in paths}
    changed = 0
    skipped = 0
    for p in paths:
        text, lines, occ = file_info(p)
        if not occ:
            continue
        by_scope = {}
        for no, col, seg, sc in occ:
            by_scope.setdefault((sc[0], sc[1], sc[2]), set()).add(seg)
        imports = []
        replace = set()
        for (start, end, indent), segs in by_scope.items():
            begin = max(start - 1, 0)
            existing = existing_names(
                "\n".join(line.rstrip("\r") for line in lines[begin:end])
            )
            for seg in segs:
                replace.add(seg)
                if seg not in existing:
                    imports.append((start, end, indent, seg))
        if not imports:
            skipped += len(occ)
            continue
        # Replace only executable code, never use declarations, comments, strings,
        # or macro bodies. `masked` preserves offsets, so the edits stay aligned.
        ml = masked(text).split("\n")
        macros = macro_spans(masked(text))
        for no, line in enumerate(lines):
            if no < len(ml) and USE.match(ml[no]):
                continue
            if any(a <= no + 1 <= b for a, b in macros):
                continue
            masked_line = ml[no] if no < len(ml) else ""
            hits = [m for m in PATH.finditer(masked_line) if m.group(1) in replace]
            for hit in reversed(hits):
                start = hit.start()
                end = start + len("crate::") + len(hit.group(1))
                line = line[:start] + hit.group(1) + line[end:]
            lines[no] = line
        # Insert each import in its own lexical module scope, bottom-up.
        imports.sort(key=lambda x: (x[0], x[1]), reverse=True)
        inserted = set()
        for start, _end, indent, seg in imports:
            key = (start, seg)
            if key in inserted:
                continue
            inserted.add(key)
            if start == 0:
                idx = 0
                while idx < len(lines) and (
                    lines[idx].startswith("//!")
                    or lines[idx].startswith("#![")
                    or lines[idx].strip() == ""
                ):
                    idx += 1
                while idx < len(lines) and (
                    lines[idx].lstrip().startswith("extern crate")
                    or lines[idx].lstrip().startswith("#[macro_use]")
                ):
                    idx += 1
                line = f"use crate::{seg};"
            else:
                idx = start
                line = " " * (indent + 4) + f"use crate::{seg};"
            lines.insert(idx, line)
        p.write_text("\n".join(lines), encoding="utf-8")
        changed += 1
    print(
        f"fixed {changed} file(s), skipped {skipped} conflicting/ambiguous occurrence(s)"
    )
    if verify and changed:
        result = subprocess.run(
            ["cargo", "check", "--workspace", "--all-targets"],
            cwd=pathlib.Path(__file__).resolve().parent.parent,
        )
        if result.returncode != 0:
            for p, text in backups.items():
                p.write_text(text, encoding="utf-8")
            print("verification failed; all changes were reverted")
            return result.returncode
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("mode", choices=["check", "fix"])
    ap.add_argument("--root", default=".")
    ap.add_argument("--no-verify", action="store_true")
    ap.add_argument("--changed", action="store_true")
    args = ap.parse_args()
    root = pathlib.Path(args.root)
    if args.changed:
        import subprocess

        try:
            names = subprocess.check_output(
                ["git", "diff", "--name-only", "HEAD"], text=True
            ).splitlines()
            if not names:
                names = subprocess.check_output(
                    ["git", "diff", "--name-only", "HEAD^", "HEAD"], text=True
                ).splitlines()
        except Exception:
            names = []
        paths = [root / n for n in names if n.endswith(".rs") and (root / n).is_file()]
    else:
        paths = (
            [root]
            if root.is_file()
            else [p for p in root.rglob("*.rs") if not (set(p.parts) & SKIP_PARTS)]
        )
    return (
        check(paths) if args.mode == "check" else fix(paths, verify=not args.no_verify)
    )


if __name__ == "__main__":
    sys.exit(main())
