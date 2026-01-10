//! Width resolution algorithm for table columns.
//!
//! This module handles calculating the actual display width for each column
//! based on the column specifications and available space.

use super::types::{TableSpec, Width};
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

impl TableSpec {
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
    /// use outstanding::table::{TableSpec, Column, Width};
    ///
    /// let spec = TableSpec::builder()
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
        let mut fill_indices: Vec<usize> = Vec::new();
        let mut used_width: usize = 0;

        // First pass: resolve Fixed and Bounded columns
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
                    fill_indices.push(i);
                }
            }
        }

        // Second pass: allocate remaining space to Fill columns only
        let remaining = available.saturating_sub(used_width);

        if !fill_indices.is_empty() {
            let per_fill = remaining / fill_indices.len();
            let mut extra = remaining % fill_indices.len();

            for &idx in &fill_indices {
                let mut width = per_fill;
                if extra > 0 {
                    width += 1;
                    extra -= 1;
                }
                widths[idx] = width;
            }
        }
        // If no Fill columns, remaining space is simply unused

        ResolvedWidths { widths }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::{Column, Width};

    #[test]
    fn resolve_empty_spec() {
        let spec = TableSpec::builder().build();
        let resolved = spec.resolve_widths(80);
        assert!(resolved.is_empty());
    }

    #[test]
    fn resolve_fixed_columns() {
        let spec = TableSpec::builder()
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
        let spec = TableSpec::builder()
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
        let spec = TableSpec::builder()
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
        let spec = TableSpec::builder()
            .column(Column::new(Width::Fill))
            .column(Column::new(Width::Fill))
            .column(Column::new(Width::Fill))
            .build();

        // Total: 10, no overhead, split 3 ways: 4, 3, 3
        let resolved = spec.resolve_widths(10);
        assert_eq!(resolved.widths, vec![4, 3, 3]);
        assert_eq!(resolved.total(), 10);
    }

    #[test]
    fn resolve_bounded_with_min() {
        let spec = TableSpec::builder()
            .column(Column::new(Width::Bounded {
                min: Some(10),
                max: None,
            }))
            .build();

        let resolved = spec.resolve_widths(80);
        assert_eq!(resolved.widths, vec![10]);
    }

    #[test]
    fn resolve_bounded_from_data() {
        let spec = TableSpec::builder()
            .column(Column::new(Width::Bounded {
                min: Some(5),
                max: Some(20),
            }))
            .column(Column::new(Width::Fixed(10)))
            .build();

        let data: Vec<Vec<&str>> = vec![
            vec!["short", "value"],
            vec!["longer text here", "x"],
        ];

        let resolved = spec.resolve_widths_from_data(80, &data);
        // "longer text here" is 16 chars, within [5, 20]
        assert_eq!(resolved.widths[0], 16);
        assert_eq!(resolved.widths[1], 10);
    }

    #[test]
    fn resolve_bounded_clamps_to_max() {
        let spec = TableSpec::builder()
            .column(Column::new(Width::Bounded {
                min: Some(5),
                max: Some(10),
            }))
            .build();

        let data: Vec<Vec<&str>> = vec![vec!["this is a very long string that exceeds max"]];

        let resolved = spec.resolve_widths_from_data(80, &data);
        assert_eq!(resolved.widths[0], 10); // Clamped to max
    }

    #[test]
    fn resolve_bounded_respects_min() {
        let spec = TableSpec::builder()
            .column(Column::new(Width::Bounded {
                min: Some(10),
                max: Some(20),
            }))
            .build();

        let data: Vec<Vec<&str>> = vec![vec!["hi"]]; // Only 2 chars

        let resolved = spec.resolve_widths_from_data(80, &data);
        assert_eq!(resolved.widths[0], 10); // Raised to min
    }

    #[test]
    fn resolve_with_decorations() {
        let spec = TableSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fill))
            .separator(" | ") // 3 chars
            .prefix("│ ")    // 2 chars
            .suffix(" │")    // 2 chars
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
        let spec = TableSpec::builder()
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
    fn resolve_no_fill_leaves_remainder_unused() {
        let spec = TableSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Bounded {
                min: Some(5),
                max: Some(30),
            }))
            .build();

        // Without data, bounded uses min (5)
        // Total: 50, available: 50, used: 15
        // No Fill column, so remaining 35 is unused
        let resolved = spec.resolve_widths(50);
        assert_eq!(resolved.widths, vec![10, 5]);
        assert_eq!(resolved.total(), 15);
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
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::table::{Column, Width};
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn resolved_widths_fit_available_space(
            num_fixed in 0usize..4,
            fixed_width in 1usize..20,
            has_fill in prop::bool::ANY,
            total_width in 20usize..200,
        ) {
            let mut builder = TableSpec::builder();

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
        ) {
            let spec = TableSpec::builder()
                .column(Column::new(Width::Bounded {
                    min: Some(min_width),
                    max: Some(max_width),
                }))
                .build();

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
            prop_assert!(
                width <= max_width,
                "Width {} should be <= max {}",
                width, max_width
            );
        }
    }
}
