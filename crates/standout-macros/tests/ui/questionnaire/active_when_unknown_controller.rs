use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.unknown-controller")]
struct UnknownController {
    /// Enabled?
    enabled: bool,

    /// Name?
    #[question(active_when(field = "ghost", is = "yes"))]
    name: Option<String>,
}

fn main() {}
