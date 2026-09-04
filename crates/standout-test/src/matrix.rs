use crate::{SnapshotCase, TestHarness};
use standout::ColorPolicy;
use standout_render::Representation;
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
#[cfg(test)]
mod tests {
    use super::*;
    const MODES: [Representation; 2] = [Representation::Human, Representation::Json];
    #[test]
    fn the_matrix_is_the_full_cross_product_in_mode_major_order() {
        let cells = matrix(&MODES, &[false, true], &[("default", 0), ("downstream", 1)]);
        assert_eq!(cells.len(), 8);
        let spelled: Vec<String> = cells
            .iter()
            .map(|c| format!("{:?}/{}/{}", c.mode, c.color, c.theme_name))
            .collect();
        assert_eq!(
            spelled,
            [
                "Human/false/default",
                "Human/false/downstream",
                "Human/true/default",
                "Human/true/downstream",
                "Json/false/default",
                "Json/false/downstream",
                "Json/true/default",
                "Json/true/downstream",
            ]
        );
    }
    #[test]
    fn every_cell_names_a_distinct_snapshot() {
        let cells = matrix(
            &MODES,
            &[false, true],
            &[("default", ()), ("downstream", ())],
        );
        let mut keys: Vec<String> = cells
            .iter()
            .map(|c| c.snapshot_case("help").key())
            .collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), 8, "two cells share a snapshot name");
    }
    #[test]
    fn a_cell_spells_its_axes_into_the_snapshot_key() {
        let cells = matrix(&[Representation::Json], &[true], &[("downstream", ())]);
        assert_eq!(
            cells[0].snapshot_case("help").key(),
            "help__mode_json__color_on__theme_downstream"
        );
    }
    #[test]
    fn the_theme_payload_rides_along() {
        let cells = matrix(&[Representation::Human], &[false], &[("a", 41), ("b", 42)]);
        assert_eq!(cells[0].theme, 41);
        assert_eq!(cells[1].theme, 42);
    }
    #[test]
    fn an_empty_axis_yields_an_empty_matrix() {
        assert!(matrix::<()>(&[], &[false], &[]).is_empty());
        assert!(matrix(&MODES, &[], &[("default", ())]).is_empty());
    }
}
