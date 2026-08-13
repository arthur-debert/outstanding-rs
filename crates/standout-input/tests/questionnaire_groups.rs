//! Pure answer-sheet engine tests for nested and repeatable groups:
//! definition validation, minimum-count rendering, copied blocks, occurrence
//! paths, malformed boundaries, cosmetic edits, structural fingerprinting,
//! conditional repeated fields, and accumulated diagnostics.

use standout_input::questionnaire::{
    AnswerSheetDiagnostic, FormError, Group, Item, Questionnaire, QuestionnaireError, ScalarField,
    ScalarKind, ValidationDiagnostic,
};

/// A nested fixture: a root field, a section, and inside the section a
/// repeatable group (minimum 1, maximum 3) with a defaulted child.
fn commands() -> Questionnaire {
    Questionnaire::new(
        "demo.commands",
        vec![
            Item::from(ScalarField::new(
                "project.name",
                "What is the project name?",
                ScalarKind::String,
            )),
            Item::from(Group::new(
                "command",
                "Describe the initial command.",
                vec![
                    Item::from(ScalarField::new(
                        "command.name",
                        "What is the command name?",
                        ScalarKind::String,
                    )),
                    Item::from(
                        Group::new(
                            "command.inputs",
                            "Describe a command input.",
                            vec![
                                ScalarField::new(
                                    "command.inputs.name",
                                    "What is its name?",
                                    ScalarKind::String,
                                ),
                                ScalarField::new(
                                    "command.inputs.value_type",
                                    "What type of value is it?",
                                    ScalarKind::String,
                                )
                                .with_default("string"),
                            ],
                        )
                        .repeatable(1)
                        .max_occurrences(3),
                    ),
                ],
            )),
        ],
    )
    .unwrap()
}

/// A repeatable fixture with conditions: a per-occurrence controller
/// (`inputs.flag` gates `inputs.flag_name`) and a root controller
/// (`advanced` gates `inputs.doc` in every occurrence).
fn conditional_inputs() -> Questionnaire {
    Questionnaire::new(
        "demo.cond",
        vec![
            Item::from(
                ScalarField::new("advanced", "Advanced mode?", ScalarKind::Bool).with_default("no"),
            ),
            Item::from(
                Group::new(
                    "inputs",
                    "Describe an input.",
                    vec![
                        ScalarField::new("inputs.name", "Name?", ScalarKind::String),
                        ScalarField::new("inputs.flag", "Expose a flag?", ScalarKind::Bool)
                            .with_default("no"),
                        ScalarField::new("inputs.flag_name", "Flag name?", ScalarKind::String)
                            .active_when("inputs.flag", "yes"),
                        ScalarField::new("inputs.doc", "Document it?", ScalarKind::String)
                            .active_when("advanced", "yes"),
                    ],
                )
                .repeatable(1),
            ),
        ],
    )
    .unwrap()
}

/// Set `answer` below the `n`th (0-based) question line tagged `id`,
/// replacing a rendered pre-filled default line when one is present.
fn answer_nth(sheet: &str, id: &str, n: usize, answer: &str) -> String {
    let tag = format!("<id:{id}>");
    let lines: Vec<&str> = sheet.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut seen = 0;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        out.push(line.to_string());
        i += 1;
        if line.trim_end().ends_with(&tag) {
            if seen == n {
                // A non-blank line right below the question is a pre-filled
                // default: the answer replaces it.
                if lines.get(i).is_some_and(|next| !next.trim().is_empty()) {
                    i += 1;
                }
                out.push(answer.to_string());
            }
            seen += 1;
        }
    }
    assert!(seen > n, "found only {seen} question lines for {tag}");
    out.join("\n") + "\n"
}

/// Split a sheet at the first question line tagged `id`: everything before
/// it, and the block from that line to the end of the sheet.
fn split_at_tag(sheet: &str, id: &str) -> (String, String) {
    let tag = format!("<id:{id}>");
    let mut offset = 0;
    for line in sheet.lines() {
        if line.trim_end().ends_with(&tag) {
            return (sheet[..offset].to_string(), sheet[offset..].to_string());
        }
        offset += line.len() + 1;
    }
    panic!("no question line found for {id}");
}

/// The `commands()` sheet with every scalar answered and one input block.
fn answered_commands_sheet(q: &Questionnaire) -> String {
    let sheet = q.render_answer_sheet();
    let sheet = answer_nth(&sheet, "project.name", 0, "demo");
    let sheet = answer_nth(&sheet, "command.name", 0, "generate");
    answer_nth(&sheet, "command.inputs.name", 0, "definition")
}

// ============================================================================
// Definition validation
// ============================================================================

#[test]
fn definition_rejects_empty_group() {
    let err = Questionnaire::new(
        "demo",
        vec![Item::from(Group::new("g", "?", Vec::<Item>::new()))],
    )
    .unwrap_err();
    assert!(err.to_string().contains("Group 'g' declares no children"));
}

#[test]
fn definition_rejects_child_id_that_does_not_extend_the_group() {
    let err = Questionnaire::new(
        "demo",
        vec![Item::from(Group::new(
            "cmd",
            "?",
            vec![ScalarField::new("name", "?", ScalarKind::String)],
        ))],
    )
    .unwrap_err();
    assert!(err
        .to_string()
        .contains("Item 'name' inside group 'cmd' must extend the group's ID ('cmd.<segment>')"));
}

#[test]
fn definition_rejects_a_zero_minimum() {
    let err = Questionnaire::new(
        "demo",
        vec![Item::from(
            Group::new(
                "g",
                "?",
                vec![ScalarField::new("g.a", "?", ScalarKind::String)],
            )
            .repeatable(0),
        )],
    )
    .unwrap_err();
    assert!(matches!(&err, QuestionnaireError::Item { id, .. } if id == "g"));
    assert!(err
        .to_string()
        .contains("Invalid repeat bounds on group 'g'"));
}

#[test]
fn definition_rejects_a_maximum_below_the_minimum() {
    let err = Questionnaire::new(
        "demo",
        vec![Item::from(
            Group::new(
                "g",
                "?",
                vec![ScalarField::new("g.a", "?", ScalarKind::String)],
            )
            .repeatable(2)
            .max_occurrences(1),
        )],
    )
    .unwrap_err();
    assert!(matches!(&err, QuestionnaireError::Item { id, .. } if id == "g"));
    assert!(err
        .to_string()
        .contains("Invalid repeat bounds on group 'g'"));
}

#[test]
fn definition_rejects_a_maximum_without_repeatable() {
    let err = Questionnaire::new(
        "demo",
        vec![Item::from(
            Group::new(
                "g",
                "?",
                vec![ScalarField::new("g.a", "?", ScalarKind::String)],
            )
            .max_occurrences(2),
        )],
    )
    .unwrap_err();
    assert!(matches!(&err, QuestionnaireError::Item { id, .. } if id == "g"));
    assert!(err
        .to_string()
        .contains("Invalid repeat bounds on group 'g'"));
}

#[test]
fn definition_rejects_duplicate_ids_across_fields_and_groups() {
    let err = Questionnaire::new(
        "demo",
        vec![
            Item::from(ScalarField::new("a.b", "?", ScalarKind::String)),
            Item::from(Group::new(
                "a",
                "?",
                vec![ScalarField::new("a.b", "?", ScalarKind::String)],
            )),
        ],
    )
    .unwrap_err();
    assert!(err.to_string().contains("Duplicate ID 'a.b'"));
}

#[test]
fn definition_rejects_a_condition_into_a_sibling_group() {
    // g2.b's controller lives inside sibling group g1: with repeats there
    // would be no single occurrence to resolve it against.
    let err = Questionnaire::new(
        "demo",
        vec![
            Item::from(Group::new(
                "g1",
                "?",
                vec![ScalarField::new("g1.a", "?", ScalarKind::Bool)],
            )),
            Item::from(Group::new(
                "g2",
                "?",
                vec![ScalarField::new("g2.b", "?", ScalarKind::String).active_when("g1.a", "yes")],
            )),
        ],
    )
    .unwrap_err();
    assert!(matches!(&err, QuestionnaireError::Item { id, .. } if id == "g2.b"));
    assert!(err
        .to_string()
        .contains("Field 'g2.b' is conditioned on 'g1.a', which is not in an enclosing scope."));
}

#[test]
fn definition_rejects_a_condition_on_a_group() {
    let err = Questionnaire::new(
        "demo",
        vec![
            Item::from(Group::new(
                "g",
                "?",
                vec![ScalarField::new("g.a", "?", ScalarKind::String)],
            )),
            Item::from(ScalarField::new("b", "?", ScalarKind::String).active_when("g", "x")),
        ],
    )
    .unwrap_err();
    assert!(matches!(&err, QuestionnaireError::Item { id, .. } if id == "b"));
    assert!(err.to_string().contains("Invalid condition on field 'b'"));
}

// ============================================================================
// Rendering
// ============================================================================

#[test]
fn render_emits_exactly_the_declared_minimum_of_blocks() {
    let q = Questionnaire::new(
        "demo",
        vec![Item::from(
            Group::new(
                "items",
                "Describe an item.",
                vec![ScalarField::new("items.name", "Name?", ScalarKind::String)],
            )
            .repeatable(2),
        )],
    )
    .unwrap();
    let sheet = q.render_answer_sheet();
    let headers = sheet
        .matches("(repeatable section, minimum 2) <id:items>")
        .count();
    assert_eq!(headers, 2, "exactly the minimum number of blocks: {sheet}");
    assert_eq!(sheet.matches("<id:items.name>").count(), 2);
}

#[test]
fn render_includes_copy_the_block_guidance_once_per_group() {
    let q = commands();
    let sheet = q.render_answer_sheet();
    assert_eq!(
        sheet.matches("copying one complete block").count(),
        1,
        "concise guidance rendered once: {sheet}"
    );
}

#[test]
fn render_numbers_nested_items_cosmetically() {
    let q = commands();
    let sheet = q.render_answer_sheet();
    assert!(sheet.contains("2. Describe the initial command. (section) <id:command>"));
    assert!(sheet.contains("2.1 What is the command name? (string) <id:command.name>"));
    assert!(sheet.contains(
        "2.2 Describe a command input. (repeatable section, minimum 1, maximum 3) <id:command.inputs>"
    ));
    assert!(sheet.contains("2.2.1 What is its name? (string) <id:command.inputs.name>"));
}

#[test]
fn render_prefills_defaults_in_every_occurrence() {
    let q = Questionnaire::new(
        "demo",
        vec![Item::from(
            Group::new(
                "items",
                "Describe an item.",
                vec![ScalarField::new("items.kind", "Kind?", ScalarKind::String)
                    .with_default("basic")],
            )
            .repeatable(2),
        )],
    )
    .unwrap();
    let sheet = q.render_answer_sheet();
    assert_eq!(sheet.matches("<id:items.kind>\nbasic\n").count(), 2);
}

#[test]
fn render_is_deterministic_for_nested_definitions() {
    let q = commands();
    assert_eq!(q.render_answer_sheet(), q.render_answer_sheet());
}

// ============================================================================
// Round trips and copied blocks
// ============================================================================

#[test]
fn minimum_sheet_round_trips_with_occurrence_paths() {
    let q = commands();
    let raw = q.parse_answer_sheet(&answered_commands_sheet(&q)).unwrap();
    assert_eq!(raw.occurrence_count("command.inputs"), 1);
    let answers = q.decode_answers(&raw).unwrap();
    assert_eq!(answers.get_text("project.name"), Some("demo"));
    assert_eq!(answers.get_text("command.name"), Some("generate"));
    assert_eq!(answers.occurrence_count("command.inputs"), 1);
    assert_eq!(
        answers.get_text("command.inputs[0].name"),
        Some("definition")
    );
    assert_eq!(
        answers.get_text("command.inputs[0].value_type"),
        Some("string"),
        "the pre-filled default decodes"
    );
}

#[test]
fn copying_a_complete_block_adds_an_occurrence() {
    let q = commands();
    let sheet = answered_commands_sheet(&q);
    let (_, block) = split_at_tag(&sheet, "command.inputs");
    let copied = format!("{sheet}\n{}", block.replace("\ndefinition\n", "\noutput\n"));

    let answers = q
        .decode_answers(&q.parse_answer_sheet(&copied).unwrap())
        .unwrap();
    assert_eq!(answers.occurrence_count("command.inputs"), 2);
    assert_eq!(
        answers.get_text("command.inputs[0].name"),
        Some("definition")
    );
    assert_eq!(answers.get_text("command.inputs[1].name"), Some("output"));
}

#[test]
fn occurrence_counts_come_from_tag_lines_not_numbers_or_wording() {
    let q = commands();
    let sheet = answered_commands_sheet(&q);
    let (_, block) = split_at_tag(&sheet, "command.inputs");
    // The copied block keeps the same display numbers, gets reworded, and
    // even claims a count in prose — none of which means anything.
    let copy = block
        .replace(
            "Describe a command input.",
            "One more input (this is the 9th of 12).",
        )
        .replace("What is its name?", "Name for this one?")
        .replace("\ndefinition\n", "\noutput\n");
    let copied = format!("{sheet}\n{copy}");

    let answers = q
        .decode_answers(&q.parse_answer_sheet(&copied).unwrap())
        .unwrap();
    assert_eq!(answers.occurrence_count("command.inputs"), 2);
    assert_eq!(answers.get_text("command.inputs[1].name"), Some("output"));
}

#[test]
fn a_root_field_after_a_group_block_closes_the_group_scope() {
    let q = Questionnaire::new(
        "demo",
        vec![
            Item::from(Group::new(
                "g",
                "Group?",
                vec![ScalarField::new("g.a", "A?", ScalarKind::String)],
            )),
            Item::from(ScalarField::new("notes", "Notes?", ScalarKind::Text).optional()),
        ],
    )
    .unwrap();
    let sheet = q.render_answer_sheet();
    let sheet = answer_nth(&sheet, "g.a", 0, "inside");
    let sheet = answer_nth(&sheet, "notes", 0, "outside");
    let raw = q.parse_answer_sheet(&sheet).unwrap();
    assert_eq!(raw.get("g.a"), Some("inside"));
    assert_eq!(raw.get("notes"), Some("outside"));
}

// ============================================================================
// Malformed boundaries
// ============================================================================

#[test]
fn a_group_tag_mentioned_inside_answer_prose_is_inert() {
    // Regression: under the old format a group ID mentioned in prose could
    // satisfy the group contract. A mid-line tag mention is now inert
    // answer content — flagged only by the tag-fragment warning.
    let q = Questionnaire::new(
        "demo",
        vec![
            Item::from(Group::new(
                "g",
                "Group?",
                vec![ScalarField::new("g.a", "A?", ScalarKind::String)],
            )),
            Item::from(ScalarField::new("notes", "Notes?", ScalarKind::Text).optional()),
        ],
    )
    .unwrap();
    let sheet = q.render_answer_sheet();
    let sheet = answer_nth(&sheet, "g.a", 0, "inside");
    let sheet = answer_nth(
        &sheet,
        "notes",
        0,
        "see <id:g> mentioned mid-line\nand more prose",
    );
    let raw = q.parse_answer_sheet(&sheet).unwrap();
    assert_eq!(
        raw.get("notes"),
        Some("see <id:g> mentioned mid-line\nand more prose")
    );
    assert_eq!(raw.get("g.a"), Some("inside"));
    assert_eq!(raw.warnings().len(), 1, "{:?}", raw.warnings());
}

#[test]
fn a_child_without_its_group_tag_line_is_misplaced() {
    let q = commands();
    let sheet = answered_commands_sheet(&q);
    // Delete the group tag line: its children now sit in the enclosing
    // scope, where they are not valid.
    let sheet: String = sheet
        .lines()
        .filter(|line| !line.contains("<id:command.inputs>"))
        .map(|line| format!("{line}\n"))
        .collect();
    let diags = q.parse_answer_sheet(&sheet).unwrap_err();
    assert!(
        diags
            .iter()
            .all(|d| matches!(d, AnswerSheetDiagnostic::Tag { message, .. } if message.contains("misplaced '<id:command.inputs."))),
        "children without their group tag line are misplaced: {diags:?}"
    );
    assert_eq!(diags.len(), 2);
}

#[test]
fn a_duplicate_section_tag_line_is_a_diagnostic() {
    let q = commands();
    let sheet = answered_commands_sheet(&q);
    let (_, block) = split_at_tag(&sheet, "command");
    let copied = format!("{sheet}\n{block}");
    let diags = q.parse_answer_sheet(&copied).unwrap_err();
    assert_eq!(
        diags.len(),
        1,
        "no cascade from the discarded copy: {diags:?}"
    );
    assert!(
        matches!(&diags[0], AnswerSheetDiagnostic::Tag { message, .. } if message.contains("duplicate group '<id:command>'")),
        "{diags:?}"
    );
}

#[test]
fn a_duplicate_child_in_one_occurrence_reports_the_indexed_path() {
    let q = commands();
    let sheet = answered_commands_sheet(&q);
    let extra = "9. Name again? (string) <id:command.inputs.name>\ntwice\n";
    let sheet = format!("{sheet}{extra}");
    let diags = q.parse_answer_sheet(&sheet).unwrap_err();
    assert_eq!(diags.len(), 1);
    assert!(
        matches!(
            &diags[0],
            AnswerSheetDiagnostic::Tag { message, .. }
                if message.contains("duplicate question '<id:command.inputs[0].name>'")
        ),
        "duplicate within an occurrence is indexed: {diags:?}"
    );
}

#[test]
fn a_bare_group_tag_line_opens_one_more_occurrence() {
    // Occurrence counting is exactly counting the group's tag lines: a
    // stray tag line opens an (empty) occurrence, whose missing required
    // children then surface as decode diagnostics — never silently.
    let q = commands();
    let sheet = answered_commands_sheet(&q);
    let sheet = format!("{sheet}\n9. Inputs again. <id:command.inputs>\n");
    let raw = q.parse_answer_sheet(&sheet).unwrap();
    assert_eq!(raw.occurrence_count("command.inputs"), 2);
    let diags = q.decode_answers(&raw).unwrap_err();
    assert_eq!(
        diags,
        vec![ValidationDiagnostic::Field {
            id: "command.inputs[1].name".into(),
            message: "this question requires an answer.".into(),
        }]
    );
}

#[test]
fn an_unknown_child_inside_a_block_is_a_diagnostic() {
    let q = conditional_inputs();
    let sheet = q.render_answer_sheet();
    let sheet = answer_nth(&sheet, "inputs.name", 0, "alpha");
    // A question line with an ID the schema never declared, inside a group
    // block: diagnosed, not silently swallowed.
    let sheet = format!("{sheet}9. Ghost? (string) <id:inputs.ghost>\nboo\n");
    let diags = q.parse_answer_sheet(&sheet).unwrap_err();
    assert_eq!(diags.len(), 1);
    assert!(
        matches!(&diags[0], AnswerSheetDiagnostic::Tag { message, .. } if message.contains("unknown question tag '<id:inputs.ghost>'")),
        "{diags:?}"
    );
}

// ============================================================================
// Decoding: bounds, indexed paths, conditions, accumulation
// ============================================================================

#[test]
fn under_minimum_occurrences_is_a_structural_diagnostic() {
    let q = commands();
    let sheet = answered_commands_sheet(&q);
    let (head, _) = split_at_tag(&sheet, "command.inputs");
    let raw = q.parse_answer_sheet(&head).unwrap();
    assert_eq!(raw.occurrence_count("command.inputs"), 0);
    let diags = q.decode_answers(&raw).unwrap_err();
    assert_eq!(
        diags,
        vec![ValidationDiagnostic::Field {
            id: "command.inputs".into(),
            message: "0 of at least 1 required item(s) submitted. Copy a complete group block - its heading line and its questions - for each missing item.".into(),
        }],
        "no speculative missing-field errors for absent occurrences: {diags:?}"
    );
}

#[test]
fn over_maximum_occurrences_is_a_structural_diagnostic() {
    let q = commands();
    let sheet = answered_commands_sheet(&q);
    let (_, block) = split_at_tag(&sheet, "command.inputs");
    let mut copied = sheet.clone();
    for n in 1..=3 {
        copied = format!(
            "{copied}\n{}",
            block.replace("\ndefinition\n", &format!("\nin{n}\n"))
        );
    }
    let diags = q
        .decode_answers(&q.parse_answer_sheet(&copied).unwrap())
        .unwrap_err();
    assert_eq!(
        diags,
        vec![ValidationDiagnostic::Field {
            id: "command.inputs".into(),
            message:
                "4 items submitted, but at most 3 are accepted. Remove the extra group block(s)."
                    .into(),
        }]
    );
}

#[test]
fn missing_required_answers_use_indexed_occurrence_paths() {
    let q = commands();
    let sheet = answered_commands_sheet(&q);
    let (_, block) = split_at_tag(&sheet, "command.inputs");
    // The copied block's name is left blank.
    let copied = format!("{sheet}\n{}", block.replace("\ndefinition\n", "\n\n"));
    let diags = q
        .decode_answers(&q.parse_answer_sheet(&copied).unwrap())
        .unwrap_err();
    assert_eq!(
        diags,
        vec![ValidationDiagnostic::Field {
            id: "command.inputs[1].name".into(),
            message: "this question requires an answer.".into(),
        }]
    );
}

#[test]
fn a_condition_inside_a_repeatable_group_controls_per_occurrence() {
    let q = conditional_inputs();
    let sheet = q.render_answer_sheet();
    let sheet = answer_nth(&sheet, "inputs.name", 0, "alpha");
    let sheet = answer_nth(&sheet, "inputs.flag", 0, "yes");
    let sheet = answer_nth(&sheet, "inputs.flag_name", 0, "alpha-flag");
    let (_, block) = split_at_tag(&sheet, "inputs");
    // Second occurrence keeps flag=no (the default), so its flag_name must
    // stay blank — and does.
    let copy = block
        .replace("\nalpha-flag\n", "\n\n")
        .replace("\nyes\n", "\nno\n")
        .replace("\nalpha\n", "\nbeta\n");
    let copied = format!("{sheet}\n{copy}");

    let answers = q
        .decode_answers(&q.parse_answer_sheet(&copied).unwrap())
        .unwrap();
    assert_eq!(answers.occurrence_count("inputs"), 2);
    assert_eq!(answers.get_text("inputs[0].flag_name"), Some("alpha-flag"));
    assert_eq!(answers.get("inputs[1].flag_name"), None);
}

#[test]
fn a_populated_inactive_field_in_an_occurrence_is_indexed() {
    let q = conditional_inputs();
    let sheet = q.render_answer_sheet();
    let sheet = answer_nth(&sheet, "inputs.name", 0, "alpha");
    // flag stays "no" (default) but flag_name is populated anyway.
    let sheet = answer_nth(&sheet, "inputs.flag_name", 0, "stale");
    let diags = q
        .decode_answers(&q.parse_answer_sheet(&sheet).unwrap())
        .unwrap_err();
    assert_eq!(
        diags,
        vec![ValidationDiagnostic::Field {
            id: "inputs[0].flag_name".into(),
            message: "this question does not apply (it is asked only when inputs.flag is true); remove its answer or change the controlling answer.".into(),
        }]
    );
}

#[test]
fn a_root_controller_gates_fields_in_every_occurrence() {
    let q = conditional_inputs();
    let sheet = q.render_answer_sheet();
    let sheet = answer_nth(&sheet, "advanced", 0, "yes");
    let sheet = answer_nth(&sheet, "inputs.name", 0, "alpha");
    let sheet = answer_nth(&sheet, "inputs.doc", 0, "documented");
    let answers = q
        .decode_answers(&q.parse_answer_sheet(&sheet).unwrap())
        .unwrap();
    assert_eq!(answers.get_text("inputs[0].doc"), Some("documented"));

    // With advanced left at its default (no), a populated doc is an error.
    let sheet = q.render_answer_sheet();
    let sheet = answer_nth(&sheet, "inputs.name", 0, "alpha");
    let sheet = answer_nth(&sheet, "inputs.doc", 0, "stale");
    let diags = q
        .decode_answers(&q.parse_answer_sheet(&sheet).unwrap())
        .unwrap_err();
    assert_eq!(
        diags,
        vec![ValidationDiagnostic::Field {
            id: "inputs[0].doc".into(),
            message: "this question does not apply (it is asked only when advanced is true); remove its answer or change the controlling answer.".into(),
        }]
    );
}

#[test]
fn diagnostics_accumulate_across_occurrences() {
    let q = conditional_inputs();
    let sheet = q.render_answer_sheet();
    // Occurrence 0: name blank (missing). Occurrence 1: flag not a bool.
    let (_, block) = split_at_tag(&sheet, "inputs");
    let copy = block
        .replace("\nno\n", "\nmaybe\n")
        .replace("Name?", "Name for the second one?");
    let copy = answer_nth(&copy, "inputs.name", 0, "beta");
    let copied = format!("{sheet}\n{copy}");

    let diags = q
        .decode_answers(&q.parse_answer_sheet(&copied).unwrap())
        .unwrap_err();
    assert_eq!(diags.len(), 2, "{diags:?}");
    assert!(diags.iter().any(
        |d| matches!(d, ValidationDiagnostic::Field { id, message } if id == "inputs[0].name" && message.contains("requires an answer"))
    ));
    assert!(diags.iter().any(
        |d| matches!(d, ValidationDiagnostic::Field { id, message } if id == "inputs[1].flag" && message.contains("yes/no"))
    ));
}

#[test]
fn whole_form_rules_run_over_indexed_answers() {
    let q = commands();
    let sheet = answered_commands_sheet(&q);
    // The copied block keeps the same name answer, violating the app rule.
    let (_, block) = split_at_tag(&sheet, "command.inputs");
    let copied = format!("{sheet}\n{block}");
    let raw = q.parse_answer_sheet(&copied).unwrap();
    let diags = q
        .decode_answers_with(&raw, |answers| {
            // An application rule spanning occurrences: input names unique.
            let count = answers.occurrence_count("command.inputs");
            let mut names: Vec<&str> = (0..count)
                .filter_map(|i| answers.get_text(&format!("command.inputs[{i}].name")))
                .collect();
            names.sort_unstable();
            names.dedup();
            if names.len() != count {
                vec![FormError::new(
                    ["command.inputs"],
                    "input names must be unique",
                )]
            } else {
                Vec::new()
            }
        })
        .unwrap_err();
    assert_eq!(diags.len(), 1);
    assert!(matches!(&diags[0], ValidationDiagnostic::Form { .. }));
}

// ============================================================================
// Fingerprint: structure and bounds are semantic; wording and order are not
// ============================================================================

/// The `commands()` fixture with different wording and item order.
fn reworded_commands() -> Questionnaire {
    Questionnaire::new(
        "demo.commands",
        vec![
            Item::from(Group::new(
                "command",
                "Tell us about your first command!",
                vec![
                    Item::from(
                        Group::new(
                            "command.inputs",
                            "An input for the command.",
                            vec![
                                ScalarField::new(
                                    "command.inputs.value_type",
                                    "Value type, please?",
                                    ScalarKind::String,
                                )
                                .with_default("string"),
                                ScalarField::new(
                                    "command.inputs.name",
                                    "Call it what?",
                                    ScalarKind::String,
                                ),
                            ],
                        )
                        .repeatable(1)
                        .max_occurrences(3),
                    ),
                    Item::from(ScalarField::new(
                        "command.name",
                        "Command name, please?",
                        ScalarKind::String,
                    )),
                ],
            )),
            Item::from(ScalarField::new(
                "project.name",
                "Project name, please?",
                ScalarKind::String,
            )),
        ],
    )
    .unwrap()
}

#[test]
fn fingerprint_ignores_wording_and_presentation_order() {
    assert_eq!(commands().fingerprint(), reworded_commands().fingerprint());
}

#[test]
fn a_reworded_sheet_still_parses_for_the_equivalent_definition() {
    // A sheet rendered by one wording parses under the other: cosmetic
    // definition changes never invalidate distributed sheets.
    let rendered_by = commands();
    let parsed_by = reworded_commands();
    let raw = parsed_by
        .parse_answer_sheet(&answered_commands_sheet(&rendered_by))
        .unwrap();
    assert_eq!(raw.get("command.inputs[0].name"), Some("definition"));
}

#[test]
fn fingerprint_covers_repeat_bounds() {
    let with_bounds = |min: usize, max: Option<usize>| {
        let mut group = Group::new(
            "items",
            "Item?",
            vec![ScalarField::new("items.name", "?", ScalarKind::String)],
        )
        .repeatable(min);
        if let Some(max) = max {
            group = group.max_occurrences(max);
        }
        Questionnaire::new("demo", vec![Item::from(group)]).unwrap()
    };
    let base = with_bounds(1, None);
    assert_ne!(base.fingerprint(), with_bounds(2, None).fingerprint());
    assert_ne!(base.fingerprint(), with_bounds(1, Some(3)).fingerprint());
}

#[test]
fn fingerprint_covers_group_structure() {
    // Same ID set, different nesting: a.c inside the group vs at the root.
    let nested = Questionnaire::new(
        "demo",
        vec![Item::from(Group::new(
            "a",
            "?",
            vec![
                ScalarField::new("a.b", "?", ScalarKind::String),
                ScalarField::new("a.c", "?", ScalarKind::String),
            ],
        ))],
    )
    .unwrap();
    let flat = Questionnaire::new(
        "demo",
        vec![
            Item::from(Group::new(
                "a",
                "?",
                vec![ScalarField::new("a.b", "?", ScalarKind::String)],
            )),
            Item::from(ScalarField::new("a.c", "?", ScalarKind::String)),
        ],
    )
    .unwrap();
    assert_ne!(nested.fingerprint(), flat.fingerprint());
}

#[test]
fn a_sheet_from_changed_bounds_is_stale() {
    let one = Questionnaire::new(
        "demo",
        vec![Item::from(
            Group::new(
                "items",
                "Item?",
                vec![ScalarField::new("items.name", "?", ScalarKind::String)],
            )
            .repeatable(1),
        )],
    )
    .unwrap();
    let two = Questionnaire::new(
        "demo",
        vec![Item::from(
            Group::new(
                "items",
                "Item?",
                vec![ScalarField::new("items.name", "?", ScalarKind::String)],
            )
            .repeatable(2),
        )],
    )
    .unwrap();
    let sheet = one.render_answer_sheet();
    let diags = two.parse_answer_sheet(&sheet).unwrap_err();
    assert!(matches!(
        &diags[..],
        [AnswerSheetDiagnostic::Incompatible { message }] if message.contains("fingerprint")
    ));
}
