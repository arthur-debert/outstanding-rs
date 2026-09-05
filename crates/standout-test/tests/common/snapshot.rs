use standout_render::Representation;
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
    /// The human representation has no `--output` spelling; the axis names it `human`.
    pub fn output_mode(self, representation: Representation) -> Self {
        self.axis("mode", output_mode_flag(representation).unwrap_or("human"))
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
pub(crate) const DIGEST_TAG: &str = "--";
pub(crate) fn slug(text: &str) -> String {
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
pub(crate) fn squash(text: &str) -> String {
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
pub(crate) fn digest(text: &str) -> u32 {
    const OFFSET_BASIS: u32 = 0x811c_9dc5;
    const PRIME: u32 = 0x0100_0193;
    let mut hash = OFFSET_BASIS;
    for byte in text.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}
macro_rules! assert_page_snapshot {
    ($result:expr, $case:expr $(,)?) => {{
        let case = $case;
        ::insta::assert_snapshot!(case.key(), $result.stdout_plain());
    }};
}
pub(crate) use assert_page_snapshot;
fn output_mode_flag(representation: Representation) -> Option<&'static str> {
    match representation {
        Representation::Human => None,
        Representation::TermDebug => Some("term-debug"),
        Representation::Json => Some("json"),
        Representation::Yaml => Some("yaml"),
        Representation::Csv => Some("csv"),
        Representation::Ndjson => Some("ndjson"),
    }
}
