use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.container", prose)]
struct ContainerProse {
    /// Name?
    name: String,
}

fn main() {}
