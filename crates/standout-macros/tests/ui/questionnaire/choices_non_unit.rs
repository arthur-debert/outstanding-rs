use standout_macros::QuestionnaireChoices;

#[derive(QuestionnaireChoices)]
enum NonUnitChoices {
    #[question(rename = "plain")]
    Plain,
    #[question(rename = "tuple")]
    Tuple(String),
}

fn main() {}
