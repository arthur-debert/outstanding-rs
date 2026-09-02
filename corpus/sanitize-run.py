#!/usr/bin/env python3
"""Sanitize a corpus run's report and transcript into a destination directory.

Applies the demo-run rules from corpus/README.md (Layout): host paths become
placeholders, session ids are zeroed, and the host's tool/plugin/connector
inventory is removed from the transcript's init event. Only report.json and
transcript.jsonl are kept — never the workspace or case sandboxes. Only
report.json is ever committed to the repository; the sanitized transcript
stays under the run's own `--out` directory.

Usage: sanitize-run.py <run-dir> <dest-dir> [--account NAME]

The workspace path, the repo checkout path, and $HOME are read from the run
itself and the environment; every occurrence in either artifact becomes
[workspace], [repo], or [home].

The host account name is never guessed, because common account names such as
`root` or `user` are ordinary evidence too. Name it with --account when the
session had a shell: `ls -l` and friends print file owners, and a bare
account name is a host identifier the committed-evidence scan rejects.
"""

import argparse
import json
import pathlib
import re

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
    repo = str(pathlib.Path(__file__).resolve().parents[1])
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


def scrub_text(text: str, subs: list[tuple[str, str]], account: str | None) -> str:
    for needle, placeholder in subs:
        text = text.replace(needle, placeholder)
    if account:
        text = re.sub(rf"\b{re.escape(account)}\b", "[user]", text)
    return re.sub(
        r'"session_id"\s*:\s*"[0-9a-f-]{36}"',
        f'"session_id":"{ZERO_SESSION}"',
        text,
        flags=re.IGNORECASE,
    )


def scrub_transcript_line(
    line: str, subs: list[tuple[str, str]], account: str | None
) -> str:
    line = scrub_text(line, subs, account)
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
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run_dir", type=pathlib.Path)
    parser.add_argument("dest", type=pathlib.Path)
    parser.add_argument("--account", help="host account name to replace with [user]")
    args = parser.parse_args()

    run_dir, dest, account = args.run_dir, args.dest, args.account
    dest.mkdir(parents=True, exist_ok=True)
    subs = replacements(run_dir)

    report = scrub_text((run_dir / "report.json").read_text(), subs, account)
    (dest / "report.json").write_text(report)

    lines = (run_dir / "transcript.jsonl").read_text().splitlines()
    scrubbed = "".join(
        scrub_transcript_line(line, subs, account) + "\n" for line in lines
    )
    (dest / "transcript.jsonl").write_text(scrubbed)
    print(f"sanitized {run_dir.name} -> {dest}")


if __name__ == "__main__":
    main()
