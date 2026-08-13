use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.conditional.required.scalar")]
struct RequiredConditionalScalar {
    /// Enabled?
    enabled: bool,

    /// Name?
    #[question(active_when(field = "enabled", is = "yes"))]
    name: String,
}

fn main() {}
