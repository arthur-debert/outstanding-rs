use std::path::PathBuf;

use standout_macros::Questionnaire;

#[derive(Questionnaire)]
#[question(id = "demo.prose")]
struct ProseNonString {
    /// Path?
    #[question(prose)]
    path: PathBuf,
}

fn main() {}
