use standout_input::questionnaire::EarlierAnswers;
use standout_macros::Questionnaire;

fn dynamic_default(_: &EarlierAnswers<'_>) -> String {
    "dynamic".to_string()
}

#[derive(Questionnaire)]
#[question(id = "demo.defaults")]
struct Demo {
    /// Name?
    #[question(default = "static", default_with = dynamic_default, revision = "1")]
    name: String,
}

fn main() {}
