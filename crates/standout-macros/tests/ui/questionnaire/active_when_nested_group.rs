use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.conditional.group")]
struct ConditionalGroup {
    /// Enabled?
    enabled: bool,

    /// Settings?
    #[question(active_when(field = "enabled", is = "yes"))]
    settings: Settings,
}

#[derive(Questionnaire)]
#[question(id = "demo.settings")]
struct Settings {
    /// Name?
    name: String,
}

fn main() {}
