use standout_macros::QuestionnaireChoices;

#[derive(QuestionnaireChoices)]
enum UnknownChoiceAttr {
    #[question(id = "custom")]
    Custom,
}

fn main() {}
