use console::Term;

pub(crate) fn probe_terminal_width() -> Option<usize> {
    resolve_terminal_width(std::env::var_os("COLUMNS").as_deref(), || {
        terminal_size::terminal_size().map(|(width, _)| width.0 as usize)
    })
}

pub(crate) fn probe_stdout_color_capability() -> bool {
    Term::stdout().features().colors_supported()
}

pub(crate) fn probe_stderr_color_capability() -> bool {
    Term::stderr().features().colors_supported()
}

fn resolve_terminal_width(
    columns: Option<&std::ffi::OsStr>,
    probe_terminal: impl FnOnce() -> Option<usize>,
) -> Option<usize> {
    columns
        .and_then(std::ffi::OsStr::to_str)
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|&width| width > 0)
        .or_else(probe_terminal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::ffi::OsStr;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn columns_values_are_resolved_without_panicking(value in any::<String>()) {
            let expected = value
                .parse::<usize>()
                .ok()
                .filter(|&width| width > 0)
                .or(Some(73));
            prop_assert_eq!(
                resolve_terminal_width(Some(OsStr::new(&value)), || Some(73)),
                expected,
            );
        }

        #[test]
        fn every_positive_columns_width_precedes_the_terminal_probe(width in 1usize..) {
            prop_assert_eq!(
                resolve_terminal_width(Some(OsStr::new(&width.to_string())), || {
                    panic!("valid COLUMNS must prevent terminal probing")
                }),
                Some(width),
            );
        }
    }
}
