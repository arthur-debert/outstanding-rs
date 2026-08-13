use standout_macros::Questionnaire;

struct GenericChoice<T>(T);

#[derive(Questionnaire)]
#[question(id = "demo.unsupported")]
struct UnsupportedGenericType {
    /// Stage?
    #[question(choice)]
    stage: GenericChoice<String>,
}

fn main() {}
