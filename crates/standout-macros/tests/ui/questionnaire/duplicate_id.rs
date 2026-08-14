use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.duplicate")]
struct DuplicateId {
    /// First?
    #[question(id = "same")]
    one: String,

    /// Second?
    #[question(id = "same")]
    two: String,
}

fn main() {}
