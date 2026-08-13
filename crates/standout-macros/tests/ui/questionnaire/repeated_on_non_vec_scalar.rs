use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.repeated")]
struct Repeated {
    /// Name?
    #[question(repeated)]
    name: String,
}

fn main() {}
