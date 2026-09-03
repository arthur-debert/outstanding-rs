#!/usr/bin/env python3
"""Compute a scorecard's objective table from committed run reports.

Usage: scorecard.py <label>=<runs-dir> [<label>=<runs-dir> ...] [--json]

Each run directory holds one report.json (plus the sanitized transcript).
One row per run, grouped by archetype so a re-run sits beside the run it is
compared with. The counting rules are the ones the pilot scorecard published,
so a set of pilot reports reproduces the pilot's figures:

- acceptance: required cases (`expected = "pass"`) and gap cases (`expected
  = "fail"`, always naming a manifest `[gaps]` entry) are two denominators,
  never one — a case suite mixes cases the framework must satisfy today with
  cases specced past it on purpose, and a single ratio of the two reads as a
  framework score no framework failure produced. A case that names a `gap`
  but has flipped to `expected = "pass"` counts as required — the gap marker
  only keeps its evidence check running, it no longer excuses the case from
  the framework's score — so a `hand-rolled-pass` outcome can land in either
  bucket. The required cell is `passed/total (%)`, `pass` popped from its
  outcome tally, with `hand-rolled-pass` broken out as `(N hand-rolled)`
  when nonzero; the gap cell, shown only when the suite has gap cases, is
  `count gap` with its own outcome tally alongside — `hand-rolled` in place
  of `hand-rolled-pass`, everything else (`pass`, `expected-fail`,
  `unexpected-pass`, `fail`) spelled out, because `unexpected-pass` is news
  about a gap closing and not an ordinary pass.
- hand_rolled_passes: gap cases whose outcome is `hand-rolled-pass` — a
  passing gap case whose manifest names evidence (`uses-crate:<name>`) the
  produced app's `Cargo.toml` lacks, so the pass was rebuilt by hand rather
  than closed by the framework. Counted separately from `acceptance`'s
  `unexpected-pass` tally, which the evidence check does not otherwise
  distinguish.
- invariants: applicable identities (every planned identity that is not
  `not-applicable`) that passed, plus the full planned breakdown, so a
  ratio can never improve by shrinking its denominator.
- workarounds: the items the exit questionnaire's `workarounds` answer
  lists. Agents list in whichever form they like — `1.`, `a)`, `(1)`,
  `(a)`, `-`, or a bold lead-in — so an item is any line that starts one,
  and only at the left margin, where an indented continuation cannot be
  mistaken for a new item. It counts what the agent listed; whether an item
  is a workaround or a deliberate application decision is a reading, and
  readings belong in the scorecard's prose beside the committed answer.
- the agent: schema 4's `provenance` block. A report written before that
  block existed states none, so the same facts are recovered from the run's
  own two sources by the rule the runner uses — the command the report
  records, split without expansion, and the same bounded head of the
  transcript, whose init event announces the backend version and the session
  model. A report whose transcript was deleted before this script could read
  it may carry that recovery already done, under `recovered_provenance`. A
  recovered agent is marked `(recovered)`: it is evidence of what answered,
  not a contemporaneous record of what was asked for. A field neither source
  states is reported unstated rather than filled in.
- comparable: whether a row is measuring the same question as the first row
  its archetype has. Two runs are comparable evidence when the spec, the
  acceptance suite, the exit questionnaire and the agent all match; where
  they do not, the row says which of them differ and a note under the table
  states the difference, so no figure reads as a framework result when a
  changed question produced it. The pinned framework version and the docs
  snapshot are the experiment's variable and are not compared.

Friction themes are not computed here. Grouping a run's frictions into
themes and ranking them is a reading of the transcripts; the script reports
how many items each run listed and the scorecard's prose does the rest.
"""

import argparse
import collections
import json
import pathlib
import re

LISTED_ITEM = re.compile(
    r"(?m)^(?:\d+[.)]\s|[a-z][.)]\s|\(\d+\)\s|\([a-z]\)\s|[-*+]\s|\*\*\S)"
)

# A hash-shaped value, which a note abbreviates rather than printing whole.
HASH = re.compile(r"(?:sha256:)?[0-9a-f]{32,}")

# What a shell would carry out and the runner will not: structure it cannot
# spawn as one process, expansion it does not perform, and globs it would
# pass through literally. Unquoted, each refuses the command outright — the
# same three sets, in the same three roles, as `session::direct_argv`.
SHELL_STRUCTURE = "|&;<>()\n"
SHELL_EXPANSION = "$`"
SHELL_GLOB = "*?[]"

# How far into a transcript the recovery reads, which is how far the runner's
# own `provenance::head` reads: a session announces itself in its first event,
# and a committed transcript is megabytes of what it did afterwards.
TRANSCRIPT_HEAD_BYTES = 256 * 1024

# What each pin makes comparable, and where the report states it.
COMPARED_PINS = (
    ("spec", "the archetype spec"),
    ("suite", "the acceptance suite"),
    ("questionnaire", "the exit questionnaire"),
)

PROVENANCE_FIELDS = (
    "backend",
    "executable_version",
    "model_requested",
    "model_observed",
    "prompt",
    "settings",
)


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
    if not report["acceptance"]["built"]:
        return "did not build"
    # A report omits `cases` when the suite produced none rather than writing
    # an empty list, so a run that built and ran nothing still reads.
    cases = report["acceptance"].get("cases", [])
    required = [case for case in cases if case["expected"] == "pass"]
    gap = [case for case in cases if case["expected"] == "fail"]

    tally = counts(case["outcome"] for case in required)
    passed = tally.pop("pass", 0)
    required_hand_rolled = tally.pop("hand-rolled-pass", 0)
    required_cell = ratio(passed, len(required))
    if required_hand_rolled:
        required_cell = f"{required_cell} ({required_hand_rolled} hand-rolled)"
    rest = ", ".join(f"{count} {outcome}" for outcome, count in sorted(tally.items()))
    if rest:
        required_cell = f"{required_cell}; {rest}"
    if not gap:
        return required_cell

    gap_tally = counts(case["outcome"] for case in gap)
    hand_rolled = gap_tally.pop("hand-rolled-pass", 0)
    gap_parts = ([f"{hand_rolled} hand-rolled"] if hand_rolled else []) + [
        f"{count} {outcome}" for outcome, count in sorted(gap_tally.items())
    ]
    gap_cell = f"{len(gap)} gap"
    if gap_parts:
        gap_cell = f"{gap_cell} ({', '.join(gap_parts)})"
    return f"{required_cell} required · {gap_cell}"


def hand_rolled_passes(report):
    cases = report["acceptance"].get("cases", [])
    return sum(1 for case in cases if case["outcome"] == "hand-rolled-pass")


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
    # A session that generated nothing is a fact about the run; only a
    # transcript the runner could not count leaves the field absent.
    generated = (
        "tokens not recorded"
        if generated is None
        else f"{generated:,} generated tokens"
    )
    return f"{wall // 60}m{wall % 60:02d}s, {generated}"


def direct_argv(agent_cmd: str) -> list[str]:
    """A recorded command's argv, split the way the runner splits one.

    This is a port of `session::direct_argv` in the runner, rule for rule:
    quotes and backslash escapes are honoured, nothing is expanded, and a
    command that would need a shell to mean what it says parses to nothing.
    The rules have to be the same rules — a command the runner would have
    refused to spawn never ran, so reading provenance out of it would invent
    a session, and a quoted `$` or `|` the runner accepts has to recover
    rather than read as a run that stated no agent. `scorecard.rs` runs one
    table of commands through both splitters.
    """
    argv: list[str] = []
    word: list[str] = []
    started = False
    chars = iter(agent_cmd)
    for char in chars:
        if char in SHELL_STRUCTURE or char in SHELL_EXPANSION or char in SHELL_GLOB:
            return []
        if char == "~" and not started:
            return []
        if char.isspace():
            if started:
                argv.append("".join(word))
                word.clear()
                started = False
            continue
        started = True
        if char == "'":
            for quoted in chars:
                if quoted == "'":
                    break
                word.append(quoted)
            else:
                return []
            continue
        if char == '"':
            for quoted in chars:
                if quoted == '"':
                    break
                if quoted == "\\":
                    escaped = next(chars, None)
                    if escaped is None:
                        return []
                    if escaped in '"\\$`':
                        word.append(escaped)
                    elif escaped != "\n":
                        # Not an escape a shell knows: both characters stand.
                        word.append("\\")
                        word.append(escaped)
                    continue
                if quoted in SHELL_EXPANSION:
                    return []
                word.append(quoted)
            else:
                return []
            continue
        if char == "\\":
            escaped = next(chars, None)
            if escaped is None:
                return []
            word.append(escaped)
            continue
        word.append(char)
    if started:
        argv.append("".join(word))
    return argv


def recorded(agent_cmd: str) -> dict:
    """What the command the report recorded says was asked, and of what."""
    argv = direct_argv(agent_cmd)
    if not argv:
        return {}
    found = {"backend": pathlib.PurePath(argv[0]).name, "settings": []}
    rest = argv[1:]
    taken = 0
    while taken < len(rest):
        argument = rest[taken]
        taken += 1
        flag, separator, inline = argument.partition("=")
        if not (separator and flag.startswith("-")):
            flag, inline = argument, None
        field = {"-p": "prompt", "--print": "prompt", "--model": "model_requested"}.get(
            flag
        )
        if field is None:
            found["settings"].append(argument)
            continue
        # Both flags take an optional value, and the flag that follows one
        # standing alone is a setting rather than its value.
        if inline is None and taken < len(rest) and not rest[taken].startswith("-"):
            inline = rest[taken]
            taken += 1
        found[field] = inline
    return found


def head(transcript: pathlib.Path) -> list[str]:
    """The transcript's leading records, whole, as far as the runner reads.

    A transcript is a whole agent session's output and the backend announces
    itself in its first event, so both readers stop once the head budget is
    spent — and both finish the record that spends it, because a record cut
    in half announces nothing.
    """
    records = []
    read = 0
    with transcript.open("rb") as file:
        for record in file:
            records.append(record.decode("utf-8", "replace"))
            read += len(record)
            if read >= TRANSCRIPT_HEAD_BYTES:
                break
    return records


def announced(transcript: pathlib.Path) -> dict:
    """What the session's transcript says answered it, by the runner's rule."""
    found = {}
    if not transcript.is_file():
        return found
    for line in head(transcript):
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
        elif event.get("type") == "assistant" and not found.get("model_observed"):
            # Not a stopping point: the runner keeps reading, because an init
            # event further in still states the version this one does not.
            found["model_observed"] = event.get("message", {}).get("model")
    return found


def provenance(report, run_dir: pathlib.Path) -> tuple[dict, bool]:
    """The agent block a report records, or one recovered from the run itself.

    The second value says which: a recovered block is evidence of what
    answered, not a contemporaneous record of what was asked for. A report
    with no committed transcript to recover from may carry that recovery
    already done, under `recovered_provenance` — backfilled once, before its
    transcript was deleted, so the report stays self-sufficient.
    """
    block = report.get("provenance")
    if block:
        return block, False
    block = report.get("recovered_provenance")
    if block:
        return block, True
    block = recorded(report["session"].get("agent_cmd", ""))
    transcript = run_dir / report["session"].get("transcript", "transcript.jsonl")
    block.update(
        {field: value for field, value in announced(transcript).items() if value}
    )
    return block, True


def stated(value) -> bool:
    return value not in (None, "", [])


def agent(block: dict, recovered: bool) -> str:
    if not any(stated(block.get(field)) for field in PROVENANCE_FIELDS):
        return "unrecorded"
    backend = block.get("backend") or "backend unstated"
    version = block.get("executable_version") or "version unstated"
    model = block.get("model_observed") or "model unstated"
    return f"{backend} {version}, {model}{' (recovered)' if recovered else ''}"


def pins(report) -> dict:
    """The pins that decide whether two runs measure the same question."""
    recorded_pins = report.get("pins", {})
    return {
        "spec": report["archetype"].get("spec_sha256"),
        "suite": recorded_pins.get("acceptance_sha256"),
        "questionnaire": recorded_pins.get("questionnaire_fingerprint"),
    }


def brief(value) -> str:
    """A value short enough for a note: a hash by its leading digits."""
    if isinstance(value, list):
        value = " ".join(value)
    if not stated(value):
        return "unstated"
    value = str(value)
    if HASH.fullmatch(value):
        return value.split(":")[-1][:8] + "…"
    return value if len(value) <= 60 else value[:59] + "…"


def difference(area: str, label: str, against, here) -> tuple[str, str] | None:
    if against == here:
        return None
    if not (stated(against) and stated(here)):
        return (
            area,
            f"{label} is stated on one side only: {brief(against)} against {brief(here)}",
        )
    return (area, f"{label}: {brief(against)} → {brief(here)}")


def differences(baseline: dict, row: dict) -> list[tuple[str, str]]:
    """How a row's question differs from the one its baseline row asked."""
    found = [
        difference(area, label, baseline["pins"][area], row["pins"][area])
        for area, label in COMPARED_PINS
    ]
    found += [
        difference(
            "agent",
            field.replace("_", " "),
            baseline["provenance"].get(field),
            row["provenance"].get(field),
        )
        for field in PROVENANCE_FIELDS
    ]
    return [note for note in found if note]


def compare(rows: list[dict]) -> None:
    """Mark every row against the first row its archetype has."""
    seen = collections.Counter(row["archetype"] for row in rows)
    baselines: dict[str, dict] = {}
    for row in rows:
        baseline = baselines.setdefault(row["archetype"], row)
        row["baseline"] = baseline["set"]
        row["differences"] = []
        if seen[row["archetype"]] == 1:
            row["comparable"] = "single run"
        elif baseline is row:
            row["comparable"] = "baseline"
        else:
            found = differences(baseline, row)
            row["differences"] = [note for _, note in found]
            areas = sorted({area for area, _ in found})
            row["comparable"] = "no: " + ", ".join(areas) if areas else "yes"


def read_runs(label: str, runs_dir: pathlib.Path) -> list[dict]:
    rows = []
    for run_dir in sorted(runs_dir.iterdir()):
        report_path = run_dir / "report.json"
        if not report_path.is_file():
            continue
        report = json.loads(report_path.read_text())
        block, recovered = provenance(report, run_dir)
        rows.append(
            {
                "set": label,
                "archetype": report["archetype"]["name"],
                "run_id": report["run_id"],
                "schema_version": report["schema_version"],
                "framework": report["pins"]["framework_version"],
                "acceptance": acceptance(report),
                "hand_rolled_passes": hand_rolled_passes(report),
                "invariants": invariants(report),
                "workarounds": workarounds(report),
                "frictions": frictions(report),
                "session": session(report),
                "agent": agent(block, recovered),
                "pins": pins(report),
                "provenance": {field: block.get(field) for field in PROVENANCE_FIELDS},
                "provenance_recovered": recovered,
            }
        )
    return rows


COLUMNS = (
    ("archetype", "Archetype"),
    ("set", "Run"),
    ("framework", "Standout"),
    ("acceptance", "Acceptance"),
    ("hand_rolled_passes", "Hand-rolled passes"),
    ("invariants", "ROB01 invariants"),
    ("workarounds", "Workarounds listed"),
    ("frictions", "Frictions listed"),
    ("session", "Session"),
    ("agent", "Agent"),
    ("comparable", "Comparable"),
)


def markdown(rows: list[dict]) -> str:
    ordered = sorted(rows, key=lambda row: (row["archetype"], row["set"]))
    header = "| " + " | ".join(title for _, title in COLUMNS) + " |"
    rule = "| " + " | ".join("---" for _ in COLUMNS) + " |"
    body = [
        "| " + " | ".join(str(row[key]) for key, _ in COLUMNS) + " |" for row in ordered
    ]
    table = "\n".join([header, rule, *body])
    notes = [
        f"- **{row['archetype']} / {row['set']}** against `{row['baseline']}` — "
        + "; ".join(row["differences"])
        for row in ordered
        if row["differences"]
    ]
    if not notes:
        return table
    return "\n".join(
        [
            table,
            "",
            "Rows marked not comparable, and what differs:",
            "",
            *notes,
        ]
    )


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
    compare(rows)

    print(json.dumps(rows, indent=2) if args.json else markdown(rows))


if __name__ == "__main__":
    main()
