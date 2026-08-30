use crate::output_mode_flag;
use standout_render::OutputMode;
use std::fmt;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotCase {
    subject: String,
    axes: Vec<(String, String)>,
}
impl SnapshotCase {
    pub fn new(subject: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
            axes: Vec::new(),
        }
    }
    pub fn axis(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.axes.push((name.into(), value.into()));
        self
    }
    pub fn output_mode(self, mode: OutputMode) -> Self {
        self.axis("mode", output_mode_flag(mode))
    }
    pub fn tty(self, is_tty: bool) -> Self {
        self.axis("tty", if is_tty { "on" } else { "off" })
    }
    pub fn color(self, color: bool) -> Self {
        self.axis("color", if color { "on" } else { "off" })
    }
    pub fn theme(self, name: impl Into<String>) -> Self {
        self.axis("theme", name)
    }
    pub fn entry_point(self, entry: impl Into<String>) -> Self {
        self.axis("entry", entry)
    }
    pub fn key(&self) -> String {
        let mut key = slug(&self.subject);
        for (name, value) in &self.axes {
            key.push_str("__");
            key.push_str(&slug(name));
            key.push('_');
            key.push_str(&slug(value));
        }
        key
    }
}
impl fmt::Display for SnapshotCase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.key())
    }
}
const DIGEST_TAG: &str = "--";
fn slug(text: &str) -> String {
    let readable = squash(text);
    if !readable.is_empty() && readable == text {
        return readable;
    }
    let base = if readable.is_empty() {
        "none"
    } else {
        &readable
    };
    format!("{}{}{:08x}", base, DIGEST_TAG, digest(text))
}
fn squash(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}
fn digest(text: &str) -> u32 {
    const OFFSET_BASIS: u32 = 0x811c_9dc5;
    const PRIME: u32 = 0x0100_0193;
    let mut hash = OFFSET_BASIS;
    for byte in text.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}
#[macro_export]
macro_rules! assert_page_snapshot {
    ($result:expr, $case:expr $(,)?) => {{
        let case = $case;
        ::insta::assert_snapshot!(case.key(), $result.stdout_plain());
    }};
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn key_is_the_subject_when_no_axis_is_recorded() {
        assert_eq!(SnapshotCase::new("help").key(), "help");
    }
    #[test]
    fn key_appends_axes_in_declaration_order() {
        let case = SnapshotCase::new("help")
            .output_mode(OutputMode::Text)
            .tty(false)
            .theme("default");
        assert_eq!(case.key(), "help__mode_text__tty_off__theme_default");
    }
    #[test]
    fn key_slugifies_argv_shaped_values() {
        let case = SnapshotCase::new("Help Page")
            .entry_point("--help")
            .output_mode(OutputMode::TermDebug);
        assert_eq!(
            case.key(),
            "help-page--f5f24815__entry_help--7fb28c5c__mode_term-debug"
        );
    }
    #[test]
    fn cases_differing_in_one_axis_differ_in_one_segment() {
        let dark = SnapshotCase::new("help").theme("dark").key();
        let light = SnapshotCase::new("help").theme("light").key();
        assert_ne!(dark, light);
        assert_eq!(dark.replace("dark", "light"), light);
    }
    #[test]
    fn values_that_squash_alike_still_key_apart() {
        let pairs = [
            ("--help", "help"),
            ("dark mode", "dark-mode"),
            ("", "none"),
            ("-h", "h"),
            ("--help", "help--7fb28c5c"),
            ("--help", "help-7fb28c5c"),
        ];
        for (left, right) in pairs {
            let left_key = SnapshotCase::new("help").entry_point(left).key();
            let right_key = SnapshotCase::new("help").entry_point(right).key();
            assert_ne!(
                left_key, right_key,
                "{:?} and {:?} must not share a snapshot name",
                left, right
            );
        }
    }
    #[test]
    fn the_axis_name_value_boundary_is_unambiguous() {
        let split_in_the_name = SnapshotCase::new("help").axis("group-1", "test").key();
        let split_in_the_value = SnapshotCase::new("help").axis("group", "1-test").key();
        assert_eq!(split_in_the_name, "help__group-1_test");
        assert_eq!(split_in_the_value, "help__group_1-test");
        assert_ne!(split_in_the_name, split_in_the_value);
    }
    #[test]
    fn a_slug_spells_the_keys_punctuation_only_in_its_reserved_tag() {
        for text in ["--help", "dark mode", "", "a__b", "x--y", "Group_1", "help"] {
            let slugged = slug(text);
            let squashed = squash(text);
            let expected_readable = if squashed.is_empty() {
                "none"
            } else {
                &squashed
            };
            let readable = match slugged.split_once(DIGEST_TAG) {
                Some((readable, tag)) => {
                    assert_eq!(
                        tag,
                        format!("{:08x}", digest(text)),
                        "{text:?} → {slugged:?}"
                    );
                    readable
                }
                None => slugged.as_str(),
            };
            assert_eq!(readable, expected_readable, "{text:?} → {slugged:?}");
            assert!(!slugged.contains('_'), "{text:?} → {slugged:?}");
            assert!(!readable.contains(DIGEST_TAG), "{text:?} → {slugged:?}");
        }
    }
    #[test]
    fn a_canonical_value_keys_without_a_digest() {
        let case = SnapshotCase::new("help")
            .output_mode(OutputMode::Text)
            .tty(true)
            .theme("solarized-dark");
        assert_eq!(case.key(), "help__mode_text__tty_on__theme_solarized-dark");
    }
    #[test]
    fn an_axis_value_that_slugs_away_keeps_the_key_unambiguous() {
        assert_eq!(
            SnapshotCase::new("help").theme("").key(),
            "help__theme_none--811c9dc5"
        );
    }
    #[test]
    fn the_digest_is_a_fixed_value_not_a_toolchain_detail() {
        assert_eq!(digest(""), 0x811c_9dc5);
        assert_eq!(format!("{:08x}", digest("--help")), "7fb28c5c");
    }
    #[test]
    fn display_renders_the_key() {
        let case = SnapshotCase::new("help").tty(true);
        assert_eq!(case.to_string(), case.key());
    }
}
