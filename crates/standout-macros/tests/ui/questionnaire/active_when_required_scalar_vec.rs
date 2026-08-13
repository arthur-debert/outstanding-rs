use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.conditional.required.scalar-vec")]
struct RequiredConditionalScalarVec {
    /// Enabled?
    enabled: bool,

    /// Tags?
    #[question(active_when(field = "enabled", is = "yes"))]
    tags: Vec<String>,
}

fn main() {}
