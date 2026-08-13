use standout_macros::QuestionnaireChoices;

#[derive(QuestionnaireChoices)]
enum NonUnitChoices {
    Plain,
    Tuple(String),
}

fn main() {}
