use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.scalar-vec")]
struct ScalarVec {
    /// Tags?
    tags: Vec<String>,
}

fn main() {}
