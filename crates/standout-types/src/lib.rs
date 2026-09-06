mod presentation;
pub mod render_data;

pub use render_data::RenderData;

pub use presentation::{
    FormattedText, InvalidStyleName, PresentationNode, PresentationStyle, SgrColor, SgrParser,
    SgrStyle, SgrToken,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Representation {
    #[default]
    Human,
    TermDebug,
    Json,
    Yaml,
    Csv,
    Ndjson,
}

impl Representation {
    pub fn is_human(&self) -> bool {
        matches!(self, Representation::Human | Representation::TermDebug)
    }

    pub fn is_debug(&self) -> bool {
        matches!(self, Representation::TermDebug)
    }

    pub fn is_structured(&self) -> bool {
        matches!(
            self,
            Representation::Json
                | Representation::Yaml
                | Representation::Csv
                | Representation::Ndjson
        )
    }

    pub fn is_stream(&self) -> bool {
        matches!(self, Representation::Ndjson)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorPolicy {
    #[default]
    Auto,
    Always,
    Never,
}
