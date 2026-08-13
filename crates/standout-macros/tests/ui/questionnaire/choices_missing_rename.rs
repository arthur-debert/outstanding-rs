use standout_macros::QuestionnaireChoices;

#[derive(QuestionnaireChoices)]
enum MissingRename {
    #[question(rename = "cli-app")]
    CliApp,
    LibraryCrate,
}

fn main() {}
