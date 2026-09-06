use super::TabularSpec;

pub trait Tabular {
    fn tabular_spec() -> TabularSpec;
}

pub trait TabularRow {
    fn to_row(&self) -> Vec<String>;
}

pub trait TabularFieldDisplay {
    fn to_tabular_cell(&self) -> String;
}

impl<T: std::fmt::Display> TabularFieldDisplay for T {
    fn to_tabular_cell(&self) -> String {
        self.to_string()
    }
}

pub trait TabularFieldOption {
    fn to_tabular_cell(&self) -> String;
}

impl<T: std::fmt::Display> TabularFieldOption for Option<T> {
    fn to_tabular_cell(&self) -> String {
        match self {
            Some(v) => v.to_string(),
            None => String::new(),
        }
    }
}
