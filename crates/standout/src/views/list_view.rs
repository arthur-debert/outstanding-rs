use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use super::{Message, MessageLevel};
use crate::tabular::TabularSpec;
use crate::ContractSurface;

#[derive(Debug, Clone)]
pub struct ListViewResult<T> {
    pub items: Vec<T>,
    pub intro: Option<String>,
    pub ending: Option<String>,
    pub messages: Vec<Message>,
    pub total_count: Option<usize>,
    pub filter_summary: Option<String>,
    pub tabular_spec: Option<TabularSpec>,
}

impl<T> ContractSurface for ListViewResult<T> {
    const SCHEMA_VERSION: u32 = 1;
}

impl<T: Serialize> Serialize for ListViewResult<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let present = [
            self.intro.is_some(),
            self.ending.is_some(),
            !self.messages.is_empty(),
            self.total_count.is_some(),
            self.filter_summary.is_some(),
            self.tabular_spec.is_some(),
        ]
        .iter()
        .filter(|set| **set)
        .count();
        let mut document = serializer.serialize_struct("ListViewResult", 2 + present)?;
        document.serialize_field("schema_version", &Self::SCHEMA_VERSION)?;
        document.serialize_field("items", &self.items)?;
        if let Some(intro) = &self.intro {
            document.serialize_field("intro", intro)?;
        }
        if let Some(ending) = &self.ending {
            document.serialize_field("ending", ending)?;
        }
        if !self.messages.is_empty() {
            document.serialize_field("messages", &self.messages)?;
        }
        if let Some(total_count) = &self.total_count {
            document.serialize_field("total_count", total_count)?;
        }
        if let Some(filter_summary) = &self.filter_summary {
            document.serialize_field("filter_summary", filter_summary)?;
        }
        if let Some(tabular_spec) = &self.tabular_spec {
            document.serialize_field("tabular_spec", tabular_spec)?;
        }
        document.end()
    }
}

impl<T> ListViewResult<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self {
            items,
            intro: None,
            ending: None,
            messages: Vec::new(),
            total_count: None,
            filter_summary: None,
            tabular_spec: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }
}

impl<T> Default for ListViewResult<T> {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

#[derive(Debug)]
pub struct ListViewBuilder<T> {
    items: Vec<T>,
    intro: Option<String>,
    ending: Option<String>,
    messages: Vec<Message>,
    total_count: Option<usize>,
    filter_summary: Option<String>,
    tabular_spec: Option<TabularSpec>,
}

impl<T> ListViewBuilder<T> {
    pub fn new(items: impl IntoIterator<Item = T>) -> Self {
        Self {
            items: items.into_iter().collect(),
            intro: None,
            ending: None,
            messages: Vec::new(),
            total_count: None,
            filter_summary: None,
            tabular_spec: None,
        }
    }

    pub fn intro(mut self, text: impl Into<String>) -> Self {
        self.intro = Some(text.into());
        self
    }

    pub fn ending(mut self, text: impl Into<String>) -> Self {
        self.ending = Some(text.into());
        self
    }

    pub fn message(mut self, level: MessageLevel, text: impl Into<String>) -> Self {
        self.messages.push(Message::new(level, text));
        self
    }

    pub fn info(self, text: impl Into<String>) -> Self {
        self.message(MessageLevel::Info, text)
    }

    pub fn success(self, text: impl Into<String>) -> Self {
        self.message(MessageLevel::Success, text)
    }

    pub fn warning(self, text: impl Into<String>) -> Self {
        self.message(MessageLevel::Warning, text)
    }

    pub fn error(self, text: impl Into<String>) -> Self {
        self.message(MessageLevel::Error, text)
    }

    pub fn total_count(mut self, count: usize) -> Self {
        self.total_count = Some(count);
        self
    }

    pub fn filter_summary(mut self, summary: impl Into<String>) -> Self {
        self.filter_summary = Some(summary.into());
        self
    }

    pub fn tabular_spec(mut self, spec: TabularSpec) -> Self {
        self.tabular_spec = Some(spec);
        self
    }

    pub fn build(self) -> ListViewResult<T> {
        ListViewResult {
            items: self.items,
            intro: self.intro,
            ending: self.ending,
            messages: self.messages,
            total_count: self.total_count,
            filter_summary: self.filter_summary,
            tabular_spec: self.tabular_spec,
        }
    }
}

pub fn list_view<T>(items: impl IntoIterator<Item = T>) -> ListViewBuilder<T> {
    ListViewBuilder::new(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_view_builder_basic() {
        let result = list_view(vec![1, 2, 3]).build();
        assert_eq!(result.items, vec![1, 2, 3]);
        assert!(result.intro.is_none());
        assert!(result.ending.is_none());
        assert!(result.messages.is_empty());
    }

    #[test]
    fn test_list_view_builder_with_intro_ending() {
        let result = list_view(vec!["a", "b"])
            .intro("Header")
            .ending("Footer")
            .build();

        assert_eq!(result.intro, Some("Header".to_string()));
        assert_eq!(result.ending, Some("Footer".to_string()));
    }

    #[test]
    fn test_list_view_builder_with_messages() {
        let result = list_view(Vec::<i32>::new())
            .info("Info message")
            .warning("Warning message")
            .error("Error message")
            .build();

        assert_eq!(result.messages.len(), 3);
        assert_eq!(result.messages[0].level, MessageLevel::Info);
        assert_eq!(result.messages[1].level, MessageLevel::Warning);
        assert_eq!(result.messages[2].level, MessageLevel::Error);
    }

    #[test]
    fn test_list_view_builder_with_filter_info() {
        let result = list_view(vec![1, 2])
            .total_count(10)
            .filter_summary("status=active")
            .build();

        assert_eq!(result.total_count, Some(10));
        assert_eq!(result.filter_summary, Some("status=active".to_string()));
    }

    #[test]
    fn test_list_view_result_len_and_empty() {
        let empty: ListViewResult<i32> = list_view(vec![]).build();
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);

        let non_empty = list_view(vec![1, 2, 3]).build();
        assert!(!non_empty.is_empty());
        assert_eq!(non_empty.len(), 3);
    }

    #[test]
    fn test_list_view_serialization() {
        let result = list_view(vec!["item1", "item2"])
            .intro("Header")
            .warning("Watch out")
            .build();

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.starts_with("{\"schema_version\":1,\"items\":[\"item1\",\"item2\"]"));
        assert!(json.contains("\"intro\":\"Header\""));
        assert!(json.contains("\"warning\""));
    }

    #[test]
    fn test_list_view_serialization_skips_empty() {
        let result = list_view(vec![1]).build();
        let json = serde_json::to_string(&result).unwrap();

        assert_eq!(json, "{\"schema_version\":1,\"items\":[1]}");
        assert!(!json.contains("\"intro\""));
        assert!(!json.contains("\"ending\""));
        assert!(!json.contains("\"messages\""));
        assert!(!json.contains("\"total_count\""));
        assert!(!json.contains("\"filter_summary\""));
        assert!(!json.contains("\"tabular_spec\""));
    }

    #[test]
    fn test_list_view_default() {
        let result: ListViewResult<String> = ListViewResult::default();
        assert!(result.is_empty());
    }
}
