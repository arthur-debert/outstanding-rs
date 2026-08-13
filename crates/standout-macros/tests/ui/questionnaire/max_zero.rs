use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.bounds")]
struct Bounds {
    /// Flags?
    #[question(repeated, max = 0)]
    flags: Vec<String>,
}

fn main() {}
