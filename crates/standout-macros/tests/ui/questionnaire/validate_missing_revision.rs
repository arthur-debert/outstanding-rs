use standout_input::questionnaire::AnswerValue;
use standout_macros::Questionnaire;

fn validate(_: &AnswerValue) -> Result<(), String> {
    Ok(())
}

#[derive(Questionnaire)]
#[question(id = "demo.validate")]
struct Demo {
    /// Name?
    #[question(validate = validate)]
    name: String,
}

fn main() {}
