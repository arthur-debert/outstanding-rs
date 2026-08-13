use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.conditional.repeated.scalar-vec")]
struct ConditionalRepeatedScalarVec {
    /// Enabled?
    enabled: bool,

    /// Flags?
    #[question(repeated, active_when(field = "enabled", is = "yes"))]
    flags: Vec<String>,
}

fn main() {}
