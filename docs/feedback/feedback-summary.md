# Documentation Feedback Summary

Consolidated feedback from external reviews of Outstanding's documentation and positioning.

---

## 1. Value Proposition: Lead with Architecture, Not Rendering

**Consensus:** Both reviewers agree the project undersells its strongest benefit.

**The Problem:**

- The README positions Outstanding as handling "boilerplate between clap and terminal output"
- This undersells what's actually ambitious: enforcing an architectural pattern with testability benefits
- The testability angle is mentioned but buried; the "rich output" features dominate

**Recommended Fix:**

- Pivot the pitch: *"Stop testing Regex on stdout. Test your Data, render your View."*
- Rewrite the opening to emphasize: "Outstanding enforces a clean separation that makes your CLI logic unit-testable as plain functions, while giving you rich terminal output, JSON/YAML modes, and hot-reloadable templates for free."
- The hook is **testing**, not **rendering**—many libs do pretty output, but few let you unit test CLI logic because it returns a `Struct`, not a `String`

---

## 2. README Quick Start Needs Context

**The Problem:**

- Jumps straight to `Output::Render(TodoResult { todos })` and `HandlerResult<TodoResult>` without explaining these types
- Someone skimming won't understand the contract immediately

**Recommended Fix:**

- Add brief inline comments or a sentence explaining the handler contract before the code
- Consider a one-liner: "Handlers receive parsed args and return data; Outstanding handles rendering"

---

## 3. Add Visual Proof

**The Problem:**

- For a library dedicated to *visuals*, the documentation is surprisingly text-heavy
- Guides describe beautiful output but force the user to imagine it

**Recommended Fix:**

- Add **screenshots or GIFs** (using `vhs` or `asciinema`) immediately visible in the README
- Use a "Before vs. After" visual:
  - Left: Messy `println!` loop with hardcoded ANSI codes
  - Right: Clean Outstanding template + struct → beautiful output
- Show actual terminal output for the tabular examples

---

## 4. Add Ecosystem Comparison

**Consensus:** Both reviewers want explicit positioning against alternatives.

**The Problem:**

- Outstanding occupies a unique niche but doesn't explain why existing tools don't solve the problem
- Users may confuse it with tools they already know

**Recommended Fix:**
Add a "Why Outstanding?" or "Why not just use X?" section:

| Tool Category | Examples | What they do | What's missing |
|---------------|----------|--------------|----------------|
| Tables only | comfy-table, tabled | Format tabular data | No dispatch, output modes, or styles |
| Terminal styling | console, termcolor | ANSI codes, colors | No templates, no JSON output |
| Rich text | termimad | Markdown-to-terminal | No clap integration, no structured output |
| Full TUI | ratatui | Interactive dashboards | Overkill for linear output, takes over screen |

**Key differentiator to emphasize:** Outstanding is a **framework**, not a widget. It handles the header, the table, the footer, and the error message with a unified theme.

---

## 5. Surface Testability with a Dedicated Guide

**The Problem:**

- The tutorial mentions testability as a benefit but doesn't demonstrate it
- This is the strongest architectural argument but has no proof

**Recommended Fix:**
Create a short guide showing:

1. How to unit-test handlers (feed fake `ArgMatches`, assert on returned data)
2. How to test templates (render with known data, assert output)
3. Example test code that would be impossible with `println!`-based code

---

## 6. Make Partial Adoption More Prominent

**The Problem:**

- For existing CLIs, the migration path is critical
- The partial adoption howto exists but is buried

**Recommended Fix:**

- Add a "Migrating an existing CLI" callout in the README
- Link prominently to partial adoption guide
- Consider a brief example in the README showing `run_to_string` with legacy fallback

---

## 7. Address Runtime Templates Trade-off

**The Problem:**

- Rust developers love compile-time safety
- Outstanding relies on runtime templates (MiniJinja)
- Users may fear template typos will crash their CLI at runtime

**Recommended Fix:**

- Explicitly address this trade-off in docs
- If compile-time template validation macros exist, highlight them
- If not, explain why flexibility (user-editable themes, hot reload) is worth the runtime check
- Show the `validate_template()` function as the mitigation strategy

---

## 8. Clarify Scope: What Outstanding Does NOT Do

**The Problem:**

- Several features are unclear or undocumented in terms of scope

**Items to clarify:**

| Topic | Question | Action |
|-------|----------|--------|
| Progress/streaming | In scope for long operations? | State explicitly if in/out of scope |
| Error output | Opinions on stderr vs stdout, exit codes? | Document error handling approach |
| Shell completions | Does it expose clap's completion generation? | Mention if supported |
| Binary output | `Binary(bytes, filename)` mentioned but not explained | Document use case (file generation?) |
| Internationalization | Template-based rendering could support i18n | Hint at roadmap if planned |

---

## 9. Polish: Fix Typos

**The Problem:**

- Visible typos undermine authority in the Rust ecosystem, which values correctness
- Examples noted: "rendetring", "deveop"

**Recommended Fix:**

- Run a strict spellcheck pass across all documentation
- High-ROI task for credibility

---

## 10. Opportunity: Clap Help Screen as Entry Point

**The Idea:**

- Most Rust CLIs use clap; the default help screen is functional but boring
- If `outstanding-clap` can make `--help` beautiful and themed with zero configuration, market that as the entry point

**Recommendation:**

- Users will install for the pretty help screen and stay for the templating engine
- Consider this as a low-friction adoption path to highlight

---

## Summary: Priority Actions

### High Priority

1. **Reposition** - Lead with testability/architecture, not just rendering
2. **Add visuals** - Screenshots/GIFs in README showing before/after
3. **Add comparison table** - Why Outstanding vs. comfy-table + termimad + manual dispatch

### Medium Priority

1. **Testability guide** - Show how to unit test handlers and templates
2. **Prominent migration path** - Surface partial adoption in README
3. **Quick Start context** - Explain HandlerResult/Output types briefly

### Lower Priority

1. **Scope documentation** - Clarify error handling, progress, completions
2. **Runtime templates** - Address compile-time safety concerns
3. **Typo pass** - Spellcheck all docs
4. **Clap help integration** - Market as entry point

---

## What's Already Working Well

- The full tutorial is excellent—incremental, respectful of developer time, with smart "Intermezzo" pauses
- The tabular guide mirrors real development progression
- Core architecture (logic/presentation split) is sound and consistently applied
- The two-pass rendering (MiniJinja → BBParser) is elegant and well-explained
- Modular structure respects that users might want renderer without arg parser
