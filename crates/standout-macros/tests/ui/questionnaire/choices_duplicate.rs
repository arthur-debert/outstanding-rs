use standout_macros::QuestionnaireChoices;

#[derive(QuestionnaireChoices)]
enum DuplicateChoices {
    One,
    #[question(rename = "one")]
    AlsoOne,
}

fn main() {}
