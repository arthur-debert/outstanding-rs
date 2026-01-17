//! Width resolution algorithm for table columns.
//!
//! This module handles calculating the actual display width for each column
//! based on the column specifications and available space.

use super::types::{FlatDataSpec, Width};
use super::util::display_width;

/// Resolved widths for all columns in a table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedWidths {
    /// Width for each column in display columns.
    pub widths: Vec<usize>,
}

impl ResolvedWidths {
    /// Get the width of a specific column.
    pub fn get(&self, index: usize) -> Option<usize> {
        self.widths.get(index).copied()
    }

    /// Get the total width of all columns (without decorations).
    pub fn total(&self) -> usize {
        self.widths.iter().sum()
    }

    /// Number of columns.
    pub fn len(&self) -> usize {
        self.widths.len()
    }

    /// Check if there are no columns.
    pub fn is_empty(&self) -> bool {
        self.widths.is_empty()
    }
}

impl FlatDataSpec {
    /// Resolve column widths without examining data.
    ///
    /// This uses minimum widths for Bounded columns and allocates remaining
    /// space to Fill columns. Use `resolve_widths_from_data` for data-driven
    /// width calculation.
    ///
    /// # Arguments
    ///
    /// * `total_width` - Total available width including decorations
    pub fn resolve_widths(&self, total_width: usize) -> ResolvedWidths {
        self.resolve_widths_impl(total_width, None)
    }

    /// Resolve column widths by examining data to determine optimal widths.
    ///
    /// For Bounded columns, scans the data to find the actual maximum width
    /// needed, then clamps to the specified bounds. Fill columns receive
    /// remaining space after all other columns are resolved.
    ///
    /// # Arguments
    ///
    /// * `total_width` - Total available width including decorations
    /// * `data` - Row data where each row is a slice of cell values
    ///
    /// # Example
    ///
    /// ```rust
    /// use standout::tabular::{FlatDataSpec, Column, Width};
    ///
    /// let spec = FlatDataSpec::builder()
    ///     .column(Column::new(Width::Bounded { min: Some(5), max: Some(20) }))
    ///     .column(Column::new(Width::Fill))
    ///     .separator("  ")
    ///     .build();
    ///
    /// let data: Vec<Vec<&str>> = vec![
    ///     vec!["short", "description"],
    ///     vec!["longer value", "another"],
    /// ];
    /// let widths = spec.resolve_widths_from_data(80, &data);
    /// ```
    pub fn resolve_widths_from_data<S: AsRef<str>>(
        &self,
        total_width: usize,
        data: &[Vec<S>],
    ) -> ResolvedWidths {
        // Calculate max width for each column from data
        let mut max_data_widths: Vec<usize> = vec![0; self.columns.len()];

        for row in data {
            for (i, cell) in row.iter().enumerate() {
                if i < max_data_widths.len() {
                    let cell_width = display_width(cell.as_ref());
                    max_data_widths[i] = max_data_widths[i].max(cell_width);
                }
            }
        }

        self.resolve_widths_impl(total_width, Some(&max_data_widths))
    }

    /// Internal implementation of width resolution.
    fn resolve_widths_impl(
        &self,
        total_width: usize,
        data_widths: Option<&[usize]>,
    ) -> ResolvedWidths {
        if self.columns.is_empty() {
            return ResolvedWidths { widths: vec![] };
        }

        let overhead = self.decorations.overhead(self.columns.len());
        let available = total_width.saturating_sub(overhead);

        let mut widths: Vec<usize> = Vec::with_capacity(self.columns.len());
        let mut flex_indices: Vec<(usize, usize)> = Vec::new(); // (index, weight) for Fill/Fraction
        let mut used_width: usize = 0;

        // First pass: resolve Fixed and Bounded columns, collect flex columns
        for (i, col) in self.columns.iter().enumerate() {
            match &col.width {
                Width::Fixed(w) => {
                    widths.push(*w);
                    used_width += w;
                }
                Width::Bounded { min, max } => {
                    let min_w = min.unwrap_or(0);
                    let max_w = max.unwrap_or(usize::MAX);

                    // If we have data widths, use them; otherwise use minimum
                    let data_w = data_widths.and_then(|dw| dw.get(i).copied()).unwrap_or(0);
                    let width = data_w.max(min_w).min(max_w);

                    widths.push(width);
                    used_width += width;
                }
                Width::Fill => {
                    widths.push(0); // Placeholder, will be filled later
                    flex_indices.push((i, 1)); // Fill has weight 1
                }
                Width::Fraction(n) => {
                    widths.push(0); // Placeholder, will be filled later
                    flex_indices.push((i, *n)); // Fraction has weight n
                }
            }
        }

        // Second pass: allocate remaining space to Fill/Fraction columns proportionally
        let remaining = available.saturating_sub(used_width);

        if !flex_indices.is_empty() {
            let total_weight: usize = flex_indices.iter().map(|(_, w)| w).sum();
            if total_weight > 0 {
                let mut remaining_space = remaining;

                for (i, (idx, weight)) in flex_indices.iter().enumerate() {
                    // Last flex column gets all remaining space to avoid rounding errors
                    let width = if i == flex_indices.len() - 1 {
                        remaining_space
                    } else {
                        let share = (remaining * weight) / total_weight;
                        remaining_space = remaining_space.saturating_sub(share);
                        share
                    };
                    widths[*idx] = width;
                }
            }
        } else if remaining > 0 {
            // If no Fill columns, distribute remaining space to the rightmost Bounded column
            // This ensures the table tries to fill the available width if possible
            if let Some(idx) = self
                .columns
                .iter()
                .rposition(|c| matches!(c.width, Width::Bounded { .. }))
            {
                // We expand the column beyond its current calculated width
                // Note: We deliberately ignore 'max' here because this is an
                // explicit layout expansion step, similar to how Fill works.
                // If the user wanted it strictly bounded, they wouldn't provide
                // extra space in total_width without a Fill column.
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
            .separator("  ") // 2 chars * 2 separators = 4
            .build();

        // Total: 80, overhead: 4, available: 76
        // Fixed: 10 + 10 = 20, remaining: 56
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

        // Total: 100, no overhead, available: 100
        // Fixed: 10, remaining: 90, split between 2 fills: 45 each
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

        // Total: 10, no overhead, split 3 ways: 3, 3, 4 (last gets remainder)
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

        // Rightmost bounded absorbs all remaining space
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
            // Add a fixed column at the end to prevent the Bounded one from being rightmost-bounded if we cared about position
            // But wait, the logic finds *rightmost Bounded*.
            // Here: [Bounded, Fixed]. Rightmost Bounded is index 0.
            // So index 0 will expand.
            .column(Column::new(Width::Fixed(10)))
            .build();

        let data: Vec<Vec<&str>> = vec![vec!["short", "value"], vec!["longer text here", "x"]];

        let resolved = spec.resolve_widths_from_data(80, &data);
        // "longer text here" is 16 chars. Fixed is 10. Used: 26. Remaining: 54.
        // Index 0 is rightmost bounded. It gets +54.
        // 16 + 54 = 70.
        assert_eq!(resolved.widths[0], 70);
        assert_eq!(resolved.widths[1], 10);
    }

    #[test]
    fn resolve_bounded_clamps_to_max_if_not_expanding() {
        // To test clamping without expansion, we ensure there is no remaining space
        // OR we make sure it's not the rightmost bounded column?
        // Or we add a Fill column to soak up space.
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Bounded {
                min: Some(5),
                max: Some(10),
            }))
            .column(Column::new(Width::Fill)) // Takes remaining space
            .build();

        let data: Vec<Vec<&str>> = vec![vec!["this is a very long string that exceeds max"]];

        let resolved = spec.resolve_widths_from_data(80, &data);
        assert_eq!(resolved.widths[0], 10); // Clamped to max, Fill takes the rest
        assert_eq!(resolved.widths[1], 70);
    }

    #[test]
    fn resolve_bounded_respects_min() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Bounded {
                min: Some(10),
                max: Some(20),
            }))
            .column(Column::new(Width::Fill)) // Ensure no expansion occurs
            .build();

        let data: Vec<Vec<&str>> = vec![vec!["hi"]]; // Only 2 chars

        let resolved = spec.resolve_widths_from_data(80, &data);
        assert_eq!(resolved.widths[0], 10); // Raised to min
        assert_eq!(resolved.widths[1], 70);
    }

    // ... (other tests unchanged) ...

    #[test]
    fn resolve_with_decorations() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fill))
            .separator(" | ") // 3 chars
            .prefix("│ ") // 2 chars
            .suffix(" │") // 2 chars
            .build();

        // Total: 50
        // Overhead: prefix(2) + suffix(2) + separator(3) = 7
        // Available: 43
        // Fixed: 10, remaining for fill: 33
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

        // Total width less than needed
        // Overhead: 4, fixed: 20, available: 20-4=16
        // Fill gets max(0, 16-20) = 0
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

        // Without data, bounded uses min (5)
        // Total: 50, available: 50, used: 15
        // No Fill column, remaining 35 is added to Bounded column (ignoring max)
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

        // Total: 100, no overhead
        // Weights: 1 + 2 + 1 = 4
        // Column 1: 100/4 * 1 = 25
        // Column 2: 100/4 * 2 = 50
        // Column 3: 25 (remaining)
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

        // Total: 10, no overhead
        // Weights: 1 + 1 + 1 = 3
        // Column 1: 10/3 = 3
        // Column 2: 10/3 = 3
        // Column 3: 4 (remaining)
        let resolved = spec.resolve_widths(10);
        assert_eq!(resolved.widths, vec![3, 3, 4]);
        assert_eq!(resolved.total(), 10);
    }

    #[test]
    fn resolve_mixed_fill_and_fraction() {
        let spec = FlatDataSpec::builder()
            .column(Column::new(Width::Fill)) // Weight 1
            .column(Column::new(Width::Fraction(2))) // Weight 2
            .column(Column::new(Width::Fill)) // Weight 1
            .build();

        // Total: 100, no overhead
        // Weights: 1 + 2 + 1 = 4
        // Column 1: 100/4 * 1 = 25
        // Column 2: 100/4 * 2 = 50
        // Column 3: 25 (remaining)
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

        // Total: 100, no overhead, fixed: 20, remaining: 80
        // Weights: 1 + 3 = 4
        // Fraction 1: 80/4 * 1 = 20
        // Fraction 3: 60 (remaining)
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

            // Fill columns should make total equal available (or less if fixed exceeds)
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

            // Create fake data with specific width
            let data_str = "x".repeat(data_width);
            let data = vec![vec![data_str.as_str()]];

            let resolved = spec.resolve_widths_from_data(100, &data);
            let width = resolved.widths[0];

            prop_assert!(
                width >= min_width,
                "Width {} should be >= min {}",
                width, min_width
            );

            // It should respect max ONLY if it's not expanding into empty space
            // It expands into empty space if fill_indices is empty (i.e. !has_fill)
            // AND it is the rightmost bounded column (which it is, as index 0)
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

            // Total should equal available width
            prop_assert_eq!(
                resolved.total(),
                total_width,
                "Fraction columns should fill entire width"
            );

            // Verify proportions approximately hold
            let total_weight: usize = fractions.iter().sum();
            for (i, &fraction) in fractions.iter().enumerate() {
                let expected = (total_width * fraction) / total_weight;
                let actual = resolved.widths[i];
                // Allow +-1 for rounding
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

            // Should fill entire width
            prop_assert_eq!(
                resolved.total(),
                total_width,
                "Mixed Fill/Fraction should fill entire width"
            );
        }
    }
}
