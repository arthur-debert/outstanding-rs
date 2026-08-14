use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.bounds")]
struct Bounds {
    /// Inputs?
    #[question(max = 0)]
    inputs: Vec<Input>,
}

#[derive(Questionnaire)]
#[question(id = "demo.input")]
struct Input {
    /// Name?
    name: String,
}

fn main() {}
