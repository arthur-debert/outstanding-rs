use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.bounds")]
struct Bounds {
    /// Inputs?
    #[question(min = 3, max = 2)]
    inputs: Vec<Input>,
}

#[derive(Questionnaire)]
#[question(id = "demo.input")]
struct Input {
    /// Name?
    name: String,
}

fn main() {}
