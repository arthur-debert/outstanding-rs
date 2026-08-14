use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.bounds")]
struct Bounds {
    /// Name?
    #[question(min = 1, max = 2)]
    name: String,
}

fn main() {}
