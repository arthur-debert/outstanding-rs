use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.enum")]
enum UnsupportedPosition {
    #[question(id = "value")]
    Value,
}

fn main() {}
