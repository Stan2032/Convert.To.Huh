#!/usr/bin/env python3
"""Validate .claude/agents/*.md against the subagent frontmatter schema.

Dependency-free. Checks the fields Claude Code actually reads, plus two
project conventions: `name` must match the filename, and the description must
be specific enough to route on.

Reference: https://code.claude.com/docs/en/sub-agents
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
AGENTS = ROOT / ".claude" / "agents"

VALID_MODELS = {"sonnet", "opus", "haiku", "fable", "inherit"}
VALID_COLORS = {"red", "blue", "green", "yellow", "purple", "orange", "pink", "cyan"}
VALID_EFFORT = {"low", "medium", "high", "xhigh", "max"}
VALID_PERMISSION_MODES = {
    "default", "acceptEdits", "auto", "dontAsk", "bypassPermissions", "plan", "manual",
}
KNOWN_FIELDS = {
    "name", "description", "tools", "disallowedTools", "model", "permissionMode",
    "maxTurns", "skills", "mcpServers", "hooks", "memory", "background", "effort",
    "isolation", "color", "initialPrompt",
}
NAME_RE = re.compile(r"^[a-z0-9]+(-[a-z0-9]+)*$")

MIN_DESCRIPTION_CHARS = 60


def main() -> int:
    if not AGENTS.is_dir():
        print(f"x {AGENTS} missing")
        return 1

    errors: list[str] = []
    warnings: list[str] = []
    seen: dict[str, str] = {}
    # README.md documents the agent set; it is not itself an agent. Claude Code
    # skips files without name/description silently, so this mirrors runtime.
    files = [p for p in sorted(AGENTS.glob("*.md")) if p.name.lower() != "readme.md"]

    for p in files:
        text = p.read_text(encoding="utf-8")
        if not text.startswith("---\n"):
            errors.append(f"{p.name}: missing YAML frontmatter")
            continue
        end = text.find("\n---\n", 3)
        if end == -1:
            errors.append(f"{p.name}: unterminated frontmatter")
            continue

        fm: dict[str, str] = {}
        for line in text[4:end].splitlines():
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            if line.startswith((" ", "\t", "-")):
                continue  # nested value; not validated here
            if ":" not in line:
                errors.append(f"{p.name}: unparseable frontmatter line: {line!r}")
                continue
            k, v = line.split(":", 1)
            fm[k.strip()] = v.strip()
        body = text[end + 5:]

        for req in ("name", "description"):
            if req not in fm:
                errors.append(f"{p.name}: missing required field `{req}` (agent is skipped silently without it)")

        name = fm.get("name", "")
        if name:
            if not NAME_RE.match(name):
                errors.append(f"{p.name}: name `{name}` must be lowercase alphanumeric with hyphens, no colons")
            if name != p.stem:
                errors.append(f"{p.name}: name `{name}` should match the filename stem `{p.stem}`")
            if name in seen:
                errors.append(f"{p.name}: duplicate name `{name}` (also in {seen[name]})")
            seen[name] = p.name

        for k in fm:
            if k not in KNOWN_FIELDS:
                warnings.append(f"{p.name}: unknown frontmatter field `{k}`")

        if (m := fm.get("model")) and m not in VALID_MODELS and not m.startswith("claude-"):
            errors.append(f"{p.name}: model `{m}` is not a known alias or model id")
        if (c := fm.get("color")) and c not in VALID_COLORS:
            errors.append(f"{p.name}: color `{c}` invalid")
        if (e := fm.get("effort")) and e not in VALID_EFFORT:
            errors.append(f"{p.name}: effort `{e}` invalid")
        if (pm := fm.get("permissionMode")) and pm not in VALID_PERMISSION_MODES:
            errors.append(f"{p.name}: permissionMode `{pm}` invalid")

        desc = fm.get("description", "")
        if len(desc) < MIN_DESCRIPTION_CHARS:
            errors.append(
                f"{p.name}: description is {len(desc)} chars; needs >= {MIN_DESCRIPTION_CHARS}. "
                "The main agent routes on this — say when to use it, not just what it is."
            )
        if not body.strip():
            errors.append(f"{p.name}: empty body — the body is the agent's system prompt")

    print(f"checked {len(files)} agent files")
    if warnings:
        print(f"\n{len(warnings)} warning(s):")
        for w in warnings:
            print(f"  ! {w}")
    if errors:
        print(f"\n{len(errors)} error(s):")
        for e in errors:
            print(f"  x {e}")
        return 1
    print("\nOK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
