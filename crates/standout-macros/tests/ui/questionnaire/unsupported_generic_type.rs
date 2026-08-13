use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.unsupported")]
struct UnsupportedGenericType {
    /// Tags?
    tags: Vec<String>,
}

fn main() {}
