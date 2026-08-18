#!/usr/bin/env python3
"""Sanitize a corpus run for committing under corpus/pilot/runs/.

Applies the demo-run rules from corpus/README.md (Layout): host paths become
placeholders, session ids are zeroed, and the host's tool/plugin/connector
inventory is removed from the transcript's init event. Only report.json and
transcript.jsonl are kept — never the workspace or case sandboxes.

Usage: sanitize-run.py <run-dir> <dest-dir>

The workspace path, the repo checkout path, and $HOME are read from the run
itself and the environment; every occurrence in either artifact becomes
[workspace], [repo], or [home].
"""

import json
import pathlib
import re
import sys

ZERO_SESSION = "00000000-0000-0000-0000-000000000000"
# Init-event keys that inventory the host rather than describe the run.
INIT_INVENTORY_KEYS = (
    "tools",
    "mcp_servers",
    "plugins",
    "slash_commands",
    "agents",
    "skills",
    "available_commands",
    "connectors",
    "memory_paths",
    "messaging_socket_path",
    "terminal_slash_commands",
)


def replacements(run_dir: pathlib.Path) -> list[tuple[str, str]]:
    workspace = str((run_dir / "workspace").resolve())
    repo = str(pathlib.Path(__file__).resolve().parents[2])
    home = str(pathlib.Path.home())
    # Sort by actual needle length so nested paths win regardless of the
    # order in which they were declared. The dash forms cover Claude Code's
    # path-encoded project ids. Do not replace the bare home-directory name:
    # common account names such as `root` or `user` are ordinary evidence too.
    paths = [
        (str(run_dir.resolve()), "[run]"),
        (workspace, "[workspace]"),
        (repo, "[repo]"),
        (repo.replace("/", "-"), "[project]"),
        (home, "[home]"),
        (home.replace("/", "-"), "[home]"),
    ]
    return sorted(paths, key=lambda item: len(item[0]), reverse=True)


def scrub_text(text: str, subs: list[tuple[str, str]]) -> str:
    for needle, placeholder in subs:
        text = text.replace(needle, placeholder)
    return re.sub(
        r'"session_id"\s*:\s*"[0-9a-f-]{36}"',
        f'"session_id":"{ZERO_SESSION}"',
        text,
        flags=re.IGNORECASE,
    )


def scrub_transcript_line(line: str, subs: list[tuple[str, str]]) -> str:
    line = scrub_text(line, subs)
    try:
        event = json.loads(line)
    except json.JSONDecodeError:
        return line
    if event.get("type") == "system" and event.get("subtype") == "init":
        for key in INIT_INVENTORY_KEYS:
            event.pop(key, None)
        return json.dumps(event, separators=(",", ":"))
    return line


def main() -> None:
    run_dir = pathlib.Path(sys.argv[1])
    dest = pathlib.Path(sys.argv[2])
    dest.mkdir(parents=True, exist_ok=True)
    subs = replacements(run_dir)

    report = scrub_text((run_dir / "report.json").read_text(), subs)
    (dest / "report.json").write_text(report)

    lines = (run_dir / "transcript.jsonl").read_text().splitlines()
    scrubbed = "".join(scrub_transcript_line(line, subs) + "\n" for line in lines)
    (dest / "transcript.jsonl").write_text(scrubbed)
    print(f"sanitized {run_dir.name} -> {dest}")


if __name__ == "__main__":
    main()
