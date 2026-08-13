use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.active")]
struct Demo {
    /// Enabled?
    enabled: bool,

    /// Name?
    #[question(active_when = "enabled")]
    name: String,
}

fn main() {}
