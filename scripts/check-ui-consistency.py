#!/usr/bin/env python3
"""Forensic UI checks that a type-checker can't make.

Written after a bulk className sweep silently ate closing quotes in three files.
TypeScript caught that one; the rest of these are design-system drift, which
nothing else catches.
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "src"

ALLOWED_RADII = {
    "rounded-full",
    "rounded-[var(--r-sm)]",
    "rounded-[var(--r-md)]",
    "rounded-[var(--r-lg)]",
    "rounded-[var(--r-xl)]",
    "rounded-[6px]",    # inline code
    "rounded-[7px]",    # checkboxes, genuinely smaller than the smallest token
    "rounded-[2.5px]",  # contribution-grid cells
}

failures: list[str] = []


def check(name: str, problems: list[str]) -> None:
    if problems:
        failures.append(name)
        print(f"  FAIL {name}")
        for p in problems[:12]:
            print(f"       {p}")
        if len(problems) > 12:
            print(f"       … and {len(problems) - 12} more")
    else:
        print(f"  ok   {name}")


files = sorted(SRC.rglob("*.tsx"))

# 1. Unterminated string literals — what a bulk className sweep once broke.
#
# Comments are stripped first: a quoted phrase wrapped across two lines of a
# JSX comment is perfectly valid but has an odd quote count on each line, and
# reporting those buries the real thing this looks for.
def without_comments(text: str) -> list[tuple[int, str]]:
    out, in_block = [], False
    for i, line in enumerate(text.splitlines(), 1):
        stripped = line
        if in_block:
            end = stripped.find("*/")
            if end == -1:
                continue
            stripped, in_block = stripped[end + 2 :], False
        # Opening a block comment (including the JSX `{/* … */}` form).
        while True:
            start = stripped.find("/*")
            if start == -1:
                break
            end = stripped.find("*/", start + 2)
            if end == -1:
                stripped, in_block = stripped[:start], True
                break
            stripped = stripped[:start] + stripped[end + 2 :]
        line_comment = stripped.find("//")
        if line_comment != -1 and stripped[:line_comment].count('"') % 2 == 0:
            stripped = stripped[:line_comment]
        out.append((i, stripped))
    return out


bad = []
for p in files:
    for i, line in without_comments(p.read_text()):
        if line.count('"') % 2 == 1:
            bad.append(f"{p.relative_to(ROOT)}:{i}")
check("no unterminated string literals", bad)

# 2. Radii must come from the token scale.
bad = []
for p in files:
    for r in re.findall(r"rounded-\[[^\]]+\]|rounded-(?:full|sm|md|lg|xl|2xl|3xl)", p.read_text()):
        if r not in ALLOWED_RADII:
            bad.append(f"{p.relative_to(ROOT)}: {r}")
check("radii come from the token scale", sorted(set(bad)))

# 3. Colour literals must be tokens, so light/dark both work.
bad = []
for p in files:
    for m in re.findall(r"#[0-9a-fA-F]{6}\b", p.read_text()):
        bad.append(f"{p.relative_to(ROOT)}: {m}")
check("no hard-coded hex colours", sorted(set(bad)))

# 4. Elevation must come from the token scale.
bad = []
for p in files:
    for m in re.findall(r"shadow-(?!\[var\(--e-)(?!none)[a-z0-9\[\]/.-]+", p.read_text()):
        bad.append(f"{p.relative_to(ROOT)}: shadow-{m.split('-', 1)[1] if '-' in m else m}")
check("shadows come from the token scale", sorted(set(bad)))

# 5. Dead UI.
bad = []
for p in files:
    t = p.read_text()
    for pattern in ["onClick={() => {}}", 'href="#"', "TODO", "FIXME", "lorem ipsum"]:
        if pattern in t:
            bad.append(f"{p.relative_to(ROOT)}: {pattern}")
check("no dead handlers or placeholders", bad)

print()
if failures:
    print(f"{len(failures)} check(s) failed")
    sys.exit(1)
print("UI consistency checks passed")
