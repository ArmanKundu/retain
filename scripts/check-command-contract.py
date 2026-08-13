#!/usr/bin/env python3
"""Verify the Tauri command contract between Rust and TypeScript.

Three things must line up or a call fails at runtime with no compile error:
  1. every `invoke("x")` in api.ts is registered in the handler,
  2. every registered command is defined with `#[tauri::command]`, and
  3. each invoke passes exactly the arguments its Rust signature declares
     (Tauri camel-cases them across the boundary).
"""
import re
import sys
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent


def camel(s: str) -> str:
    head, *rest = s.split("_")
    return head + "".join(w.capitalize() for w in rest)


def split_top(s: str, sep: str = ",") -> list[str]:
    """Split on `sep` at bracket depth zero."""
    out, cur, depth = [], "", 0
    for c in s:
        if c in "<([{":
            depth += 1
        elif c in ">)]}":
            depth -= 1
        if c == sep and depth == 0:
            out.append(cur)
            cur = ""
        else:
            cur += c
    out.append(cur)
    return [x.strip() for x in out if x.strip()]


def span(text: str, start: int, open_c: str, close_c: str) -> int:
    """Index just past the bracket that opens at `start`."""
    depth = 0
    i = start
    while i < len(text):
        if text[i] == open_c:
            depth += 1
        elif text[i] == close_c:
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return -1


def rust_commands() -> dict[str, list[str]]:
    src = (ROOT / "src-tauri/src/commands.rs").read_text()
    out = {}
    for m in re.finditer(r"#\[tauri::command\]\s*\n\s*pub (?:async )?fn ([a-z0-9_]+)\s*\(", src):
        open_paren = src.index("(", m.end() - 1)
        close = span(src, open_paren, "(", ")")
        params = []
        for p in split_top(src[open_paren + 1 : close]):
            ident = p.split(":", 1)[0].strip()
            # State/handle injections aren't sent from TypeScript.
            if ident in ("state", "app", "window", "self"):
                continue
            if not re.fullmatch(r"[a-z_][a-z0-9_]*", ident):
                continue
            params.append(ident)
        out[m.group(1)] = params
    return out


def ts_invocations() -> list[tuple[str, set[str]]]:
    src = (ROOT / "src/lib/api.ts").read_text()
    calls = []
    for m in re.finditer(r'invoke(?:<[^;]*?>)?\(', src):
        open_paren = m.end() - 1
        close = span(src, open_paren, "(", ")")
        # Only this call's own arguments — not the next one's.
        inner = src[open_paren + 1 : close]
        parts = split_top(inner)
        name = parts[0].strip().strip('"')
        keys = set()
        if len(parts) > 1 and parts[1].lstrip().startswith("{"):
            obj = parts[1].strip()
            for field in split_top(obj[1:-1]):
                keys.add(field.split(":", 1)[0].strip())
        calls.append((name, keys))
    return calls


def registered() -> set[str]:
    src = (ROOT / "src-tauri/src/lib.rs").read_text()
    block = src.split("generate_handler![")[1]
    block = block[: span(block, block.index("["), "[", "]") if "[" in block else len(block)]
    return set(re.findall(r"commands::([a-z0-9_]+)", src.split("generate_handler![")[1].split("])")[0]))


def main() -> int:
    rust = rust_commands()
    reg = registered()
    calls = ts_invocations()
    invoked = {n for n, _ in calls}
    failures = []

    print(f"invoked {len(invoked)} | registered {len(reg)} | defined {len(rust)}")

    for label, diff in [
        ("invoked from TS but not registered", invoked - reg),
        ("registered but not defined", reg - set(rust)),
        ("defined but not registered (unreachable)", set(rust) - reg),
        ("registered but never called from TS (dead)", reg - invoked),
    ]:
        if diff:
            failures.append(f"{label}: {sorted(diff)}")
            print(f"  FAIL {label}: {sorted(diff)}")
        else:
            print(f"  ok   {label}: none")

    for name, keys in calls:
        if name not in rust:
            continue
        expected = {camel(p) for p in rust[name]}
        if expected != keys:
            failures.append(f"{name}: rust wants {sorted(expected)}, ts sends {sorted(keys)}")
            print(f"  FAIL {name}: rust wants {sorted(expected)}, ts sends {sorted(keys)}")

    if failures:
        print(f"\n{len(failures)} contract problem(s)")
        return 1
    print("  ok   every invoke passes exactly the arguments its command declares")
    return 0


if __name__ == "__main__":
    sys.exit(main())
