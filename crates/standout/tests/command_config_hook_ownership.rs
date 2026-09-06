use std::fs;
use std::path::PathBuf;

const OWNER: &str = "crates/standout/src/cli/group.rs";
const IMPL: &str = "impl<H> CommandConfig<H> {";

const HOOK_API: &[&str] = &["hooks", "pre_dispatch", "post_dispatch", "post_output"];

const OUTPUT_TRANSFORM_SUGAR: &[&str] = &[
    "pipe_to_with_timeout",
    "pipe_through_with_timeout",
    "pipe_to_clipboard",
    "pipe_with",
];

const CAPABILITIES: &[&str] = &[
    "questionnaire",
    "questionnaire_with_form",
    "questionnaire_with_form_and_review",
    "input",
    "without_config",
];

const REACHES: &[&str] = &[
    "self.hooks",
    "self.pre_dispatch(",
    "self.post_dispatch(",
    "self.post_output(",
];

struct Scan {
    methods: Vec<String>,
    offenders: Vec<String>,
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn method_name(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("    pub fn ")
        .or_else(|| line.strip_prefix("    fn "))?;
    let end = rest.find(['<', '('])?;
    Some(&rest[..end])
}

fn scan_of(relative: &str, source: &str) -> Scan {
    let mut methods = Vec::new();
    let mut offenders = Vec::new();
    let mut inside = false;
    let mut method: Option<&str> = None;

    for (index, line) in source.lines().enumerate() {
        if !inside {
            inside = line == IMPL;
            continue;
        }
        if line == "}" {
            inside = false;
            continue;
        }
        if let Some(name) = method_name(line) {
            method = Some(name);
            methods.push(name.to_string());
        }
        let Some(name) = method else { continue };
        if HOOK_API.contains(&name) || OUTPUT_TRANSFORM_SUGAR.contains(&name) {
            continue;
        }
        if REACHES.iter().any(|needle| line.contains(needle)) {
            offenders.push(format!(
                "{relative}:{}: `{name}`: {}",
                index + 1,
                line.trim()
            ));
        }
    }

    Scan { methods, offenders }
}

#[test]
fn only_the_hook_api_registers_through_the_application_hooks_slot() {
    let source = fs::read_to_string(workspace_root().join(OWNER)).unwrap();
    let scan = scan_of(OWNER, &source);

    let unread: Vec<&str> = HOOK_API
        .iter()
        .chain(OUTPUT_TRANSFORM_SUGAR)
        .chain(CAPABILITIES)
        .copied()
        .filter(|name| !scan.methods.iter().any(|seen| seen == name))
        .collect();
    assert!(
        unread.is_empty(),
        "this guard reads `{OWNER}` from the line `{IMPL}` to the next line that is `}}`, and that \
         range did not contain {unread:?}. A where-clause on the impl, a moved brace or a renamed \
         method leaves the scan with nothing to check and the assertion below passes vacuously; \
         re-anchor the scan on the current source."
    );

    assert!(
        scan.offenders.is_empty(),
        "`CommandConfig::hooks` is the application's registration, and a framework capability \
         that writes into it spends the command's registration for that phase: `build()` then \
         refuses a command that declares the capability and an `AppBuilder::hooks` hook for the \
         same phase, naming a hook the command never wrote. Give the capability its own slot on \
         `CommandConfig` the way `input_chains` and `questionnaire_resolution` have one, threaded \
         through its own `ErasedCommandConfig::take_*` into its own map on `AppBuilder`.\n{}",
        scan.offenders.join("\n")
    );
}

#[test]
fn the_scan_names_a_capability_that_registers_through_the_hook_api() {
    let source = "\
impl<H> CommandConfig<H> {
    pub fn questionnaire<T>(mut self) -> Self {
        self.questionnaire = Some(QuestionnaireCommand::new::<T>());
        self.pre_dispatch(move |matches, ctx| resolve(matches, ctx))
    }

    pub fn survey(mut self) -> Self {
        self.hooks = Some(Hooks::default());
        self
    }

    pub fn pre_dispatch<F>(mut self, f: F) -> Self {
        let hooks = self.hooks.take().unwrap_or_default();
        self.hooks = Some(hooks.pre_dispatch(f));
        self
    }
}

impl<F, T> ErasedCommandConfig for ClosureCommandConfig<F, T> {
    fn take_hooks(&mut self) -> Option<Hooks> {
        self.hooks.take()
    }
}
";
    let scan = scan_of("group.rs", source);
    assert_eq!(scan.methods, ["questionnaire", "survey", "pre_dispatch"]);
    assert_eq!(
        scan.offenders,
        [
            "group.rs:4: `questionnaire`: self.pre_dispatch(move |matches, ctx| resolve(matches, ctx))",
            "group.rs:8: `survey`: self.hooks = Some(Hooks::default());",
        ]
    );
}

#[test]
fn a_drifted_impl_header_leaves_the_scan_with_no_methods() {
    let source = "\
impl<H> CommandConfig<H>
where
    H: Clone,
{
    pub fn questionnaire<T>(mut self) -> Self {
        self.pre_dispatch(move |matches, ctx| resolve(matches, ctx))
    }
}
";
    let scan = scan_of("group.rs", source);
    assert!(scan.methods.is_empty());
    assert!(scan.offenders.is_empty());
}
