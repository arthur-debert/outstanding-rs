use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.vec")]
struct UnsupportedVec {
    /// Counts?
    counts: Vec<usize>,
}

fn main() {}
