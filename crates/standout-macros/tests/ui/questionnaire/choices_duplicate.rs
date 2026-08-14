use standout_macros::QuestionnaireChoices;

#[derive(QuestionnaireChoices)]
enum DuplicateChoices {
    #[question(rename = "one")]
    One,
    #[question(rename = "one")]
    AlsoOne,
}

fn main() {}
