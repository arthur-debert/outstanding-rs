use super::SnapshotCase;
use standout::ColorPolicy;
use standout_render::Representation;
use standout_test::TestHarness;
#[derive(Debug, Clone)]
pub struct MatrixCell<T> {
    pub mode: Representation,
    pub color: bool,
    pub theme_name: String,
    pub theme: T,
}
impl<T> MatrixCell<T> {
    pub fn harness(&self) -> TestHarness {
        TestHarness::new()
            .output_mode(self.mode)
            .color(if self.color {
                ColorPolicy::Always
            } else {
                ColorPolicy::Never
            })
    }
    pub fn snapshot_case(&self, subject: impl Into<String>) -> SnapshotCase {
        SnapshotCase::new(subject)
            .output_mode(self.mode)
            .color(self.color)
            .theme(&self.theme_name)
    }
}
pub fn matrix<T: Clone>(
    modes: &[Representation],
    colors: &[bool],
    themes: &[(&str, T)],
) -> Vec<MatrixCell<T>> {
    let mut cells = Vec::with_capacity(modes.len() * colors.len() * themes.len());
    for &mode in modes {
        for &color in colors {
            for (name, theme) in themes {
                cells.push(MatrixCell {
                    mode,
                    color,
                    theme_name: (*name).to_string(),
                    theme: theme.clone(),
                });
            }
        }
    }
    cells
}
