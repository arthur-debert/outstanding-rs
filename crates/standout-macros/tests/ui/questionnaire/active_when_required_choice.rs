use standout_macros::{Questionnaire, QuestionnaireChoices};

#[derive(QuestionnaireChoices)]
enum Mode {
    Local,
    Docker,
}

#[derive(Questionnaire)]
#[question(id = "demo.conditional.required.choice")]
struct RequiredConditionalChoice {
    /// Enabled?
    enabled: bool,

    /// Mode?
    #[question(choice, active_when(field = "enabled", is = "yes"))]
    mode: Mode,
}

fn main() {}
