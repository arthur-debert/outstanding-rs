use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.cross-struct")]
struct Root {
    /// Enabled?
    enabled: bool,

    /// Child?
    child: Child,
}

#[derive(Questionnaire)]
#[question(id = "demo.cross-struct.child")]
struct Child {
    /// Name?
    #[question(active_when(field = "enabled", is = "yes"))]
    name: Option<String>,
}

fn main() {}
