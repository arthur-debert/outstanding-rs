use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.conditional.repeated.group")]
struct ConditionalRepeatedGroup {
    /// Enabled?
    enabled: bool,

    /// Steps?
    #[question(active_when(field = "enabled", is = "yes"))]
    steps: Vec<Step>,
}

#[derive(Questionnaire)]
#[question(id = "demo.step")]
struct Step {
    /// Name?
    name: String,
}

fn main() {}
