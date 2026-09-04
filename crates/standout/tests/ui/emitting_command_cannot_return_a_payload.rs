// A command that declares events returns `Summary`, which has no payload
// variant, so `Binary` and `Artifact` are refused where they are written
// rather than after the events have reached stdout (ADR-0041).
use standout::cli::{Artifact, Output, Results, Summary};
use standout_macros::handler;

#[derive(serde::Serialize)]
struct Step {
    name: &'static str,
}

#[handler]
fn returns_an_output(results: &mut Results<Step>) -> Result<Output<()>, anyhow::Error> {
    results.emit(Step { name: "one" })?;
    Ok(Output::Binary {
        data: vec![0],
        filename: "out.bin".into(),
    })
}

#[handler]
fn returns_an_artifact_summary(
    results: &mut Results<Step>,
) -> Result<Summary<serde_json::Value>, anyhow::Error> {
    results.emit(Step { name: "one" })?;
    Ok(Summary::Artifact(Artifact::new(vec![0])))
}

fn main() {}
