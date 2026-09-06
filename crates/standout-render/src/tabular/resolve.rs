use super::types::{FlatDataSpec, Width};
use super::util::visible_width_with_policy;
use crate::template::presentation::escape_text;
use crate::AmbiguousWidth;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedWidths {
    pub widths: Vec<usize>,
}

impl ResolvedWidths {
    pub fn get(&self, index: usize) -> Option<usize> {
        self.widths.get(index).copied()
    }

    pub fn total(&self) -> usize {
        self.widths.iter().sum()
    }

    pub fn len(&self) -> usize {
        self.widths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.widths.is_empty()
    }
}

impl FlatDataSpec {
    pub fn resolve_widths(&self, total_width: usize) -> ResolvedWidths {
        self.resolve_widths_with_policy(total_width, AmbiguousWidth::Narrow)
    }

    pub fn resolve_widths_with_policy(
        &self,
        total_width: usize,
        policy: AmbiguousWidth,
    ) -> ResolvedWidths {
        self.prepared_text()
            .resolve_prepared_widths(total_width, policy)
    }

    pub(crate) fn resolve_prepared_widths(
        &self,
        total_width: usize,
        policy: AmbiguousWidth,
    ) -> ResolvedWidths {
        self.resolve_widths_impl(total_width, None, policy)
    }

    pub fn resolve_widths_from_data<S: AsRef<str>>(
        &self,
        total_width: usize,
        data: &[Vec<S>],
    ) -> ResolvedWidths {
        self.resolve_widths_from_data_with_policy(total_width, data, AmbiguousWidth::Narrow)
    }

    pub fn resolve_widths_from_data_with_policy<S: AsRef<str>>(
        &self,
        total_width: usize,
        data: &[Vec<S>],
        policy: AmbiguousWidth,
    ) -> ResolvedWidths {
        let data: Vec<Vec<_>> = data
            .iter()
            .map(|row| {
                row.iter()
                    .map(|value| escape_text(value.as_ref()))
                    .collect()
            })
            .collect();
        self.prepared_text()
            .resolve_prepared_widths_from_data(total_width, &data, policy)
    }

    pub(crate) fn resolve_prepared_widths_from_data<S: AsRef<str>>(
        &self,
        total_width: usize,
        data: &[Vec<S>],
        policy: AmbiguousWidth,
    ) -> ResolvedWidths {
        let measured = self.measure_columns(data, policy);
        self.resolve_prepared_widths_measured(total_width, &measured, policy)
    }

    pub(crate) fn measure_columns<S: AsRef<str>>(
        &self,
        data: &[Vec<S>],
        policy: AmbiguousWidth,
    ) -> Vec<usize> {
        let mut max_data_widths: Vec<usize> = vec![0; self.columns.len()];

        for row in data {
            for (i, cell) in row.iter().enumerate() {
                if i < max_data_widths.len() {
                    let cell_width = visible_width_with_policy(cell.as_ref(), policy);
                    max_data_widths[i] = max_data_widths[i].max(cell_width);
                }
            }
        }

        max_data_widths
    }

    pub(crate) fn resolve_prepared_widths_measured(
        &self,
        total_width: usize,
        measured: &[usize],
        policy: AmbiguousWidth,
    ) -> ResolvedWidths {
        self.resolve_widths_impl(total_width, Some(measured), policy)
    }

    fn resolve_widths_impl(
        &self,
        total_width: usize,
        data_widths: Option<&[usize]>,
        policy: AmbiguousWidth,
    ) -> ResolvedWidths {
        if self.columns.is_empty() {
            return ResolvedWidths { widths: vec![] };
        }

        let overhead = self
            .decorations
            .prepared_overhead(self.columns.len(), policy);
        let available = total_width.saturating_sub(overhead);

        let mut widths: Vec<usize> = Vec::with_capacity(self.columns.len());
        let mut flex_indices: Vec<(usize, usize)> = Vec::new();
        let mut used_width: usize = 0;

        for (i, col) in self.columns.iter().enumerate() {
            match &col.width {
                Width::Fixed(w) => {
                    widths.push(*w);
                    used_width += w;
                }
                Width::Bounded { min, max } => {
                    let min_w = min.unwrap_or(0);
                    let max_w = max.unwrap_or(usize::MAX);

                    let data_w = data_widths.and_then(|dw| dw.get(i).copied()).unwrap_or(0);
                    let width = data_w.max(min_w).min(max_w);

                    widths.push(width);
                    used_width += width;
                }
                Width::Fill => {
                    widths.push(0);
                    flex_indices.push((i, 1));
                }
                Width::Fraction(n) => {
                    widths.push(0);
                    flex_indices.push((i, *n));
                }
            }
        }

        let remaining = available.saturating_sub(used_width);

        if !flex_indices.is_empty() {
            let total_weight: usize = flex_indices.iter().map(|(_, w)| w).sum();
            if total_weight > 0 {
                let mut remaining_space = remaining;

                for (i, (idx, weight)) in flex_indices.iter().enumerate() {
                    let width = if i == flex_indices.len() - 1 {
                        remaining_space
                    } else {
                        let share = (remaining * weight)
                            .checked_div(total_weight)
                            .expect("total_weight > 0 is guaranteed by the enclosing guard");
                        remaining_space = remaining_space.saturating_sub(share);
                        share
                    };
                    widths[*idx] = width;
                }
            }
        } else if remaining > 0 {
            if let Some(idx) = self
                .columns
                .iter()
                .rposition(|c| matches!(c.width, Width::Bounded { .. }))
            {
                widths[idx] += remaining;
            }
        }

        ResolvedWidths { widths }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabular::{Column, Width};

    #[test]
    fn resolve_empty_spec() {
        let spec = FlatDataSpec::builder().build();
        let resolved = spec.resolve_widths(80);
        assert!(resolved.is_empty());
    }

    #[test]
    fn resolve_fixed_columns() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(20)))
            .column(Column::new(Width::Fixed(15)))
            .build();

        let resolved = spec.resolve_widths(100);
        assert_eq!(resolved.widths, vec![10, 20, 15]);
        assert_eq!(resolved.total(), 45);
    }

    #[test]
    fn resolve_fill_column() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fill))
            .column(Column::new(Width::Fixed(10)))
            .separator("  ")
            .build();

        let resolved = spec.resolve_widths(80);
        assert_eq!(resolved.widths, vec![10, 56, 10]);
    }

    #[test]
    fn resolve_multiple_fill_columns() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fill))
            .column(Column::new(Width::Fill))
            .build();

        let resolved = spec.resolve_widths(100);
        assert_eq!(resolved.widths, vec![10, 45, 45]);
    }

    #[test]
    fn resolve_fill_columns_uneven_split() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fill))
            .column(Column::new(Width::Fill))
            .column(Column::new(Width::Fill))
            .build();

        let resolved = spec.resolve_widths(10);
        assert_eq!(resolved.widths, vec![3, 3, 4]);
        assert_eq!(resolved.total(), 10);
    }

    #[test]
    fn resolve_bounded_with_min() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Bounded {
                min: Some(10),
                max: None,
            }))
            .build();

        let resolved = spec.resolve_widths(80);
        assert_eq!(resolved.widths, vec![80]);
    }

    #[test]
    fn resolve_bounded_from_data() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Bounded {
                min: Some(5),
                max: Some(20),
            }))
            .column(Column::new(Width::Fixed(10)))
            .build();

        let data: Vec<Vec<&str>> = vec![vec!["short", "value"], vec!["longer text here", "x"]];

        let resolved = spec.resolve_widths_from_data(80, &data);
        assert_eq!(resolved.widths[0], 70);
        assert_eq!(resolved.widths[1], 10);
    }

    #[test]
    fn resolve_bounded_clamps_to_max_if_not_expanding() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Bounded {
                min: Some(5),
                max: Some(10),
            }))
            .column(Column::new(Width::Fill))
            .build();

        let data: Vec<Vec<&str>> = vec![vec!["this is a very long string that exceeds max"]];

        let resolved = spec.resolve_widths_from_data(80, &data);
        assert_eq!(resolved.widths[0], 10);
        assert_eq!(resolved.widths[1], 70);
    }

    #[test]
    fn resolve_bounded_respects_min() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Bounded {
                min: Some(10),
                max: Some(20),
            }))
            .column(Column::new(Width::Fill))
            .build();

        let data: Vec<Vec<&str>> = vec![vec!["hi"]];

        let resolved = spec.resolve_widths_from_data(80, &data);
        assert_eq!(resolved.widths[0], 10);
        assert_eq!(resolved.widths[1], 70);
    }

    #[test]
    fn resolve_with_decorations() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fill))
            .separator(" | ")
            .prefix("│ ")
            .suffix(" │")
            .build();

        let resolved = spec.resolve_widths(50);
        assert_eq!(resolved.widths, vec![10, 33]);
    }

    #[test]
    fn resolve_tight_space() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fill))
            .column(Column::new(Width::Fixed(10)))
            .separator("  ")
            .build();

        let resolved = spec.resolve_widths(24);
        assert_eq!(resolved.widths, vec![10, 0, 10]);
    }

    #[test]
    fn resolve_no_fill_expands_rightmost_bounded() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Bounded {
                min: Some(5),
                max: Some(30),
            }))
            .build();

        let resolved = spec.resolve_widths(50);
        assert_eq!(resolved.widths, vec![10, 40]);
        assert_eq!(resolved.total(), 50);
    }

    #[test]
    fn resolved_widths_accessors() {
        let resolved = ResolvedWidths {
            widths: vec![10, 20, 30],
        };

        assert_eq!(resolved.get(0), Some(10));
        assert_eq!(resolved.get(1), Some(20));
        assert_eq!(resolved.get(2), Some(30));
        assert_eq!(resolved.get(3), None);
        assert_eq!(resolved.total(), 60);
        assert_eq!(resolved.len(), 3);
        assert!(!resolved.is_empty());
    }

    #[test]
    fn resolve_fraction_columns() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fraction(1)))
            .column(Column::new(Width::Fraction(2)))
            .column(Column::new(Width::Fraction(1)))
            .build();

        let resolved = spec.resolve_widths(100);
        assert_eq!(resolved.widths, vec![25, 50, 25]);
        assert_eq!(resolved.total(), 100);
    }

    #[test]
    fn resolve_fraction_uneven_split() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fraction(1)))
            .column(Column::new(Width::Fraction(1)))
            .column(Column::new(Width::Fraction(1)))
            .build();

        let resolved = spec.resolve_widths(10);
        assert_eq!(resolved.widths, vec![3, 3, 4]);
        assert_eq!(resolved.total(), 10);
    }

    #[test]
    fn resolve_mixed_fill_and_fraction() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fill))
            .column(Column::new(Width::Fraction(2)))
            .column(Column::new(Width::Fill))
            .build();

        let resolved = spec.resolve_widths(100);
        assert_eq!(resolved.widths, vec![25, 50, 25]);
        assert_eq!(resolved.total(), 100);
    }

    #[test]
    fn resolve_fraction_with_fixed() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(20)))
            .column(Column::new(Width::Fraction(1)))
            .column(Column::new(Width::Fraction(3)))
            .build();

        let resolved = spec.resolve_widths(100);
        assert_eq!(resolved.widths, vec![20, 20, 60]);
        assert_eq!(resolved.total(), 100);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::tabular::{Column, Width};
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn resolved_widths_fit_available_space(
            num_fixed in 0usize..4,
            fixed_width in 1usize..20,
            has_fill in prop::bool::ANY,
            total_width in 20usize..200,
        ) {
            let mut builder = FlatDataSpec::builder();

            for _ in 0..num_fixed {
                builder = builder.column(Column::new(Width::Fixed(fixed_width)));
            }

            if has_fill {
                builder = builder.column(Column::new(Width::Fill));
            }

            builder = builder.separator("  ");
            let spec = builder.build();

            if spec.columns.is_empty() {
                return Ok(());
            }

            let resolved = spec.resolve_widths(total_width);
            let overhead = spec.decorations.overhead(spec.num_columns());
            let available = total_width.saturating_sub(overhead);

            if has_fill {
                let fixed_total: usize = (0..num_fixed).map(|_| fixed_width).sum();
                if fixed_total <= available {
                    prop_assert_eq!(
                        resolved.total(),
                        available,
                        "With fill column, total should equal available space"
                    );
                }
            }
        }

        #[test]
        fn bounded_columns_respect_bounds(
            min_width in 1usize..10,
            max_width in 10usize..30,
            data_width in 0usize..50,
            has_fill in prop::bool::ANY,
        ) {
            let mut builder = FlatDataSpec::builder()
                .column(Column::new(Width::Bounded {
                    min: Some(min_width),
                    max: Some(max_width),
                }));

            if has_fill {
                builder = builder.column(Column::new(Width::Fill));
            }

            let spec = builder.build();

            let data_str = "x".repeat(data_width);
            let data = vec![vec![data_str.as_str()]];

            let resolved = spec.resolve_widths_from_data(100, &data);
            let width = resolved.widths[0];

            prop_assert!(
                width >= min_width,
                "Width {} should be >= min {}",
                width, min_width
            );

            if has_fill {
                prop_assert!(
                    width <= max_width,
                    "Width {} should be <= max {} (when fill exists)",
                    width, max_width
                );
            }
        }

        #[test]
        fn fraction_columns_proportional(
            fractions in proptest::collection::vec(1usize..5, 1..5),
            total_width in 50usize..200,
        ) {
            let mut builder = FlatDataSpec::builder();
            for f in &fractions {
                builder = builder.column(Column::new(Width::Fraction(*f)));
            }
            let spec = builder.build();

            let resolved = spec.resolve_widths(total_width);

            prop_assert_eq!(
                resolved.total(),
                total_width,
                "Fraction columns should fill entire width"
            );

            let total_weight: usize = fractions.iter().sum();
            for (i, &fraction) in fractions.iter().enumerate() {
                let expected = (total_width * fraction) / total_weight;
                let actual = resolved.widths[i];
                prop_assert!(
                    actual >= expected.saturating_sub(1) && actual <= expected + fractions.len(),
                    "Column {} with weight {} should be ~{}, got {}",
                    i, fraction, expected, actual
                );
            }
        }

        #[test]
        fn mixed_fraction_and_fill_fills_space(
            num_fill in 1usize..3,
            num_fraction in 1usize..3,
            fraction_weight in 1usize..5,
            total_width in 50usize..200,
        ) {
            let mut builder = FlatDataSpec::builder();

            for _ in 0..num_fill {
                builder = builder.column(Column::new(Width::Fill));
            }
            for _ in 0..num_fraction {
                builder = builder.column(Column::new(Width::Fraction(fraction_weight)));
            }

            let spec = builder.build();
            let resolved = spec.resolve_widths(total_width);

            prop_assert_eq!(
                resolved.total(),
                total_width,
                "Mixed Fill/Fraction should fill entire width"
            );
        }
    }
}
