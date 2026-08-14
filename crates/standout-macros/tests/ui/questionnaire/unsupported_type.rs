use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.unsupported")]
struct UnsupportedType {
    /// How many?
    count: u32,
}

fn main() {}
