#!/usr/bin/env python3
"""Compute a scorecard's objective table from committed run reports.

Usage: scorecard.py <label>=<runs-dir> [<label>=<runs-dir> ...] [--json]

Each run directory holds one report.json (plus the sanitized transcript).
One row per run, grouped by archetype so a re-run sits beside the run it is
compared with. The counting rules are the ones the ROB03 pilot scorecard
published, so a set of pilot reports reproduces the pilot's figures:

- acceptance: cases whose outcome is `pass`, over every case in the suite;
  the other outcomes are spelled out rather than folded in, because
  `unexpected-pass` is news about a gap and not a passing case.
- invariants: applicable identities (every planned identity that is not
  `not-applicable`) that passed, plus the full planned breakdown, so a
  ratio can never improve by shrinking its denominator.
- workarounds: the items the exit questionnaire's `workarounds` answer
  lists. Agents list in whichever form they like — `1.`, `a)`, `-`, or a
  bold lead-in — so an item is any line that starts one, and only at the
  left margin, where an indented continuation cannot be mistaken for a new
  item. It counts what the agent listed; whether an item is a workaround or
  a deliberate application decision is a reading, and readings belong in the
  scorecard's prose beside the committed answer.
- the agent: schema 4's `provenance` block. A report written before that
  block existed states none, so the same facts are recovered from the run's
  committed transcript by the rule the runner itself uses — the init event
  announces the backend version and the session model. A recovered agent is
  marked `(from transcript)`: it is evidence of what answered, not a
  contemporaneous record of what was asked for.

Friction themes are not computed here. Grouping a run's frictions into
themes and ranking them is a reading of the transcripts; the script reports
how many items each run listed and the scorecard's prose does the rest.
"""

import argparse
import json
import pathlib
import re

LISTED_ITEM = re.compile(r"(?m)^(?:\d+[.)]\s|[a-z][.)]\s|[-*+]\s|\*\*\S)")


def counts(values):
    tally = {}
    for value in values:
        tally[value] = tally.get(value, 0) + 1
    return tally


def ratio(part, whole):
    if not whole:
        return "n/a"
    return f"{part}/{whole} ({part / whole * 100:.1f}%)"


def acceptance(report):
    cases = report["acceptance"]["cases"]
    if not report["acceptance"]["built"]:
        return "did not build"
    tally = counts(case["outcome"] for case in cases)
    passed = tally.pop("pass", 0)
    cell = ratio(passed, len(cases))
    rest = ", ".join(f"{count} {outcome}" for outcome, count in sorted(tally.items()))
    return f"{cell}; {rest}" if rest else cell


def invariants(report):
    tally = counts(cell["status"] for cell in report["invariants"])
    planned = len(report["invariants"])
    applicable = planned - tally.get("not-applicable", 0)
    breakdown = ", ".join(
        f"{tally[status]} {label}"
        for status, label in (
            ("pass", "pass"),
            ("fail", "fail"),
            ("not-run", "not run"),
            ("not-applicable", "N/A"),
        )
        if tally.get(status)
    )
    return (
        f"{ratio(tally.get('pass', 0), applicable)} applicable; "
        f"{planned} planned: {breakdown}"
    )


def workarounds(report):
    answer = report["questionnaire"]["answers"].get("workarounds", "")
    return len(LISTED_ITEM.findall(answer))


def frictions(report):
    answer = report["questionnaire"]["answers"].get("friction", "")
    return len(LISTED_ITEM.findall(answer))


def session(report):
    wall = round(report["session"]["wall_seconds"])
    generated = report["session"].get("output_tokens")
    generated = (
        f"{generated:,} generated tokens" if generated else "tokens not recorded"
    )
    return f"{wall // 60}m{wall % 60:02d}s, {generated}"


def announced(transcript: pathlib.Path) -> dict:
    """What the session's transcript says ran it, by the runner's own rule."""
    found = {}
    if not transcript.is_file():
        return found
    with transcript.open() as lines:
        for line in lines:
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                continue
            if event.get("type") == "system" and event.get("subtype") == "init":
                found = {
                    "executable_version": event.get("claude_code_version"),
                    "model_observed": event.get("model"),
                }
                if found.get("model_observed"):
                    return found
            if event.get("type") == "assistant" and not found.get("model_observed"):
                found["model_observed"] = event.get("message", {}).get("model")
    return found


def agent(report, run_dir: pathlib.Path):
    provenance = report.get("provenance")
    recovered = ""
    if not provenance:
        transcript = run_dir / report["session"].get("transcript", "transcript.jsonl")
        provenance = announced(transcript)
        recovered = " (from transcript)"
    backend = provenance.get("backend") or "claude"
    version = provenance.get("executable_version") or "version unstated"
    model = provenance.get("model_observed") or "model unstated"
    return f"{backend} {version}, {model}{recovered}"


def read_runs(label: str, runs_dir: pathlib.Path) -> list[dict]:
    rows = []
    for run_dir in sorted(runs_dir.iterdir()):
        report_path = run_dir / "report.json"
        if not report_path.is_file():
            continue
        report = json.loads(report_path.read_text())
        rows.append(
            {
                "set": label,
                "archetype": report["archetype"]["name"],
                "run_id": report["run_id"],
                "schema_version": report["schema_version"],
                "framework": report["pins"]["framework_version"],
                "acceptance": acceptance(report),
                "invariants": invariants(report),
                "workarounds": workarounds(report),
                "frictions": frictions(report),
                "session": session(report),
                "agent": agent(report, run_dir),
            }
        )
    return rows


COLUMNS = (
    ("archetype", "Archetype"),
    ("set", "Run"),
    ("framework", "Standout"),
    ("acceptance", "Acceptance"),
    ("invariants", "ROB01 invariants"),
    ("workarounds", "Workarounds listed"),
    ("frictions", "Frictions listed"),
    ("session", "Session"),
    ("agent", "Agent"),
)


def markdown(rows: list[dict]) -> str:
    header = "| " + " | ".join(title for _, title in COLUMNS) + " |"
    rule = "| " + " | ".join("---" for _ in COLUMNS) + " |"
    body = [
        "| " + " | ".join(str(row[key]) for key, _ in COLUMNS) + " |"
        for row in sorted(rows, key=lambda row: (row["archetype"], row["set"]))
    ]
    return "\n".join([header, rule, *body])


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("sets", nargs="+", metavar="LABEL=RUNS_DIR")
    parser.add_argument("--json", action="store_true", help="emit rows as JSON")
    args = parser.parse_args()

    rows = []
    for spec in args.sets:
        label, _, path = spec.partition("=")
        if not path:
            parser.error(f"expected LABEL=RUNS_DIR, got {spec!r}")
        rows.extend(read_runs(label, pathlib.Path(path)))

    print(json.dumps(rows, indent=2) if args.json else markdown(rows))


if __name__ == "__main__":
    main()
