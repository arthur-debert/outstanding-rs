//! Archetype manifest loading: `corpus/archetypes/<name>/manifest.toml`,
//! schema in `corpus/README.md`'s "Manifest format" section. `smoke` carries
//! no manifest (README's Layout exception); every roster archetype does.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Context};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub archetype: ManifestArchetype,
    pub features: Features,
    pub interactions: Vec<Interaction>,
    #[serde(default)]
    pub gaps: BTreeMap<String, GapEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestArchetype {
    pub name: String,
    pub survey: String,
    pub summary: String,
    pub status: Status,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    #[serde(rename = "in-capability")]
    InCapability,
    #[serde(rename = "partially-past-capability")]
    PartiallyPastCapability,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Features {
    pub used: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interaction {
    pub id: String,
    pub stresses: Vec<String>,
    pub description: String,
    pub cases: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum GapEntry {
    Text(String),
    Evidenced { text: String, evidence: String },
}

impl GapEntry {
    pub fn evidence(&self) -> Option<Evidence<'_>> {
        match self {
            GapEntry::Text(_) => None,
            GapEntry::Evidenced { evidence, .. } => Evidence::parse(evidence),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence<'a> {
    UsesCrate(&'a str),
}

impl<'a> Evidence<'a> {
    fn parse(text: &'a str) -> Option<Self> {
        text.strip_prefix("uses-crate:").map(Evidence::UsesCrate)
    }

    pub fn satisfied_by(&self, cargo_toml: &str) -> bool {
        let Evidence::UsesCrate(name) = self;
        let Ok(doc) = cargo_toml.parse::<toml::Value>() else {
            return false;
        };
        let Some(deps) = doc
            .as_table()
            .and_then(|root| root.get("dependencies"))
            .and_then(toml::Value::as_table)
        else {
            return false;
        };
        deps.iter().any(|(key, spec)| {
            key == name
                || spec
                    .as_table()
                    .and_then(|t| t.get("package"))
                    .and_then(toml::Value::as_str)
                    == Some(*name)
        })
    }
}

impl Manifest {
    pub fn load(archetypes_dir: &Path, name: &str) -> anyhow::Result<Self> {
        let path = archetypes_dir.join(name).join("manifest.toml");
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let manifest: Manifest =
            toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        for (gap, entry) in &manifest.gaps {
            if let GapEntry::Evidenced { evidence, .. } = entry {
                if Evidence::parse(evidence).is_none() {
                    bail!(
                        "{}: gap {gap:?} has unrecognized evidence {evidence:?}; expected \
                         `uses-crate:<name>`",
                        path.display()
                    );
                }
            }
        }
        Ok(manifest)
    }

    pub fn load_optional(archetypes_dir: &Path, name: &str) -> anyhow::Result<Option<Self>> {
        if !archetypes_dir.join(name).join("manifest.toml").is_file() {
            return Ok(None);
        }
        Self::load(archetypes_dir, name).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_parses_uses_crate() {
        assert_eq!(
            Evidence::parse("uses-crate:clapfig"),
            Some(Evidence::UsesCrate("clapfig"))
        );
        assert_eq!(Evidence::parse("clapfig"), None);
    }

    #[test]
    fn evidence_is_satisfied_by_a_plain_or_renamed_dependency() {
        let evidence = Evidence::UsesCrate("clapfig");
        assert!(evidence.satisfied_by("[dependencies]\nclapfig = \"0.24\"\n"));
        assert!(evidence.satisfied_by(
            "[dependencies]\nclapfig2 = { version = \"0.24\", package = \"clapfig\" }\n"
        ));
        assert!(!evidence.satisfied_by("[dependencies]\nserde = \"1\"\n"));
        assert!(!evidence.satisfied_by("[dependencies]\n"));
        assert!(!evidence.satisfied_by("not valid toml {"));
    }

    #[test]
    fn malformed_evidence_is_a_load_error() {
        let dir = tempfile::tempdir().unwrap();
        let archetype_dir = dir.path().join("fake");
        std::fs::create_dir_all(&archetype_dir).unwrap();
        std::fs::write(
            archetype_dir.join("manifest.toml"),
            r#"
[archetype]
name = "fake"
survey = "C1"
summary = "one line"
status = "partially-past-capability"

[features]
used = ["dispatch.subcommands"]

[[interactions]]
id = "x"
stresses = ["a", "b"]
description = "d"
cases = []

[gaps]
PAR01 = { text = "t", evidence = "clapfig" }
"#,
        )
        .unwrap();
        let err = Manifest::load(dir.path(), "fake").unwrap_err();
        assert!(err.to_string().contains("unrecognized evidence"), "{err:#}");
    }

    #[test]
    fn missing_manifest_loads_optional_as_none() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("smoke")).unwrap();
        assert!(Manifest::load_optional(dir.path(), "smoke")
            .unwrap()
            .is_none());
    }
}
