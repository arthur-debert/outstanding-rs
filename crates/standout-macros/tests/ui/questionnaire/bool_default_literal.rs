use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.bool")]
struct BoolDefaultLiteral {
    /// Continue?
    #[question(default = "maybe")]
    yes: bool,
}

fn main() {}
