use super::*;

impl Object for TabularFormatter {
    fn get_value(self: &Arc<Self>, key: &Value) -> Option<Value> {
        match key.as_str()? {
            "num_columns" => Some(Value::from(self.num_columns())),
            "widths" => {
                let widths: Vec<Value> = self.widths.iter().map(|&w| Value::from(w)).collect();
                Some(Value::from(widths))
            }
            "separator" => Some(fragment(self.separator.clone())),
            _ => None,
        }
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Str(&["num_columns", "widths", "separator"])
    }

    fn call_method(
        self: &Arc<Self>,
        _state: &minijinja::State,
        name: &str,
        args: &[Value],
    ) -> Result<Value, minijinja::Error> {
        match name {
            "row" => {
                if args.is_empty() {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::MissingArgument,
                        "row() requires an array of values",
                    ));
                }

                let values_arg = &args[0];
                let has_sub_columns = self.columns.iter().any(|c| c.sub_columns.is_some());

                if has_sub_columns {
                    let outer_iter = match values_arg.try_iter() {
                        Ok(iter) => iter,
                        Err(_) => {
                            let values = vec![stringify(values_arg).into_owned()];
                            let formatted = self.format_markup_row(&values);
                            return Ok(fragment(formatted));
                        }
                    };

                    let mut owned_values: Vec<OwnedCellValue> = Vec::new();
                    for (i, v) in outer_iter.enumerate() {
                        let is_sub_col = self
                            .columns
                            .get(i)
                            .and_then(|c| c.sub_columns.as_ref())
                            .is_some();

                        if is_sub_col {
                            if let Ok(inner_iter) = v.try_iter() {
                                let sub_vals: Vec<String> =
                                    inner_iter.map(|iv| stringify(&iv).into_owned()).collect();
                                owned_values.push(OwnedCellValue::Sub(sub_vals));
                            } else {
                                owned_values
                                    .push(OwnedCellValue::Single(stringify(&v).into_owned()));
                            }
                        } else {
                            owned_values.push(OwnedCellValue::Single(stringify(&v).into_owned()));
                        }
                    }

                    let cell_values: Vec<_> = owned_values
                        .iter()
                        .map(OwnedCellValue::as_borrowed)
                        .collect();

                    let formatted = self.format_markup_row_cells(&cell_values);
                    Ok(fragment(formatted))
                } else {
                    let values: Vec<String> = match values_arg.try_iter() {
                        Ok(iter) => iter.map(|v| stringify(&v).into_owned()).collect(),
                        Err(_) => vec![stringify(values_arg).into_owned()],
                    };

                    let formatted = self.format_markup_row(&values);
                    Ok(fragment(formatted))
                }
            }
            "row_from" => {
                if args.is_empty() {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::MissingArgument,
                        "row_from() requires an object argument",
                    ));
                }

                let value =
                    crate::RenderData::from_template_value(args[0].clone()).map_err(|error| {
                        minijinja::Error::new(
                            minijinja::ErrorKind::InvalidOperation,
                            error.to_string(),
                        )
                    })?;
                Ok(fragment(self.row_from(&value)))
            }
            "column_width" => {
                if args.is_empty() {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::MissingArgument,
                        "column_width() requires an index argument",
                    ));
                }

                let index = args[0].as_usize().ok_or_else(|| {
                    minijinja::Error::new(
                        minijinja::ErrorKind::InvalidOperation,
                        "column_width() index must be a number",
                    )
                })?;

                match self.column_width(index) {
                    Some(w) => Ok(Value::from(w)),
                    None => Ok(Value::from(())),
                }
            }
            _ => Err(minijinja::Error::new(
                minijinja::ErrorKind::UnknownMethod,
                format!("TabularFormatter has no method '{}'", name),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabular::formatter::tests::simple_spec;
    use crate::tabular::{TabularSpec, Width};

    #[test]
    fn object_get_num_columns() {
        let formatter = Arc::new(TabularFormatter::new(&simple_spec(), 80));
        let value = formatter.get_value(&Value::from("num_columns"));
        assert_eq!(value, Some(Value::from(2)));
    }

    #[test]
    fn object_get_widths() {
        let spec = TabularSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(8)))
            .build();
        let formatter = Arc::new(TabularFormatter::new(&spec, 80));

        let value = formatter.get_value(&Value::from("widths"));
        assert!(value.is_some());
        let widths = value.unwrap();
        assert!(widths.try_iter().is_ok());
    }

    #[test]
    fn object_get_separator() {
        let spec = TabularSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .separator(" | ")
            .build();
        let formatter = Arc::new(TabularFormatter::new(&spec, 80));

        let value = formatter.get_value(&Value::from("separator"));
        assert_eq!(value.unwrap().to_string(), " | ");
    }

    #[test]
    fn object_get_unknown_returns_none() {
        let formatter = Arc::new(TabularFormatter::new(&simple_spec(), 80));
        let value = formatter.get_value(&Value::from("unknown"));
        assert_eq!(value, None);
    }

    #[test]
    fn object_row_method_via_template() {
        let spec = TabularSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(8)))
            .separator(" | ")
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let mut env = crate::template::new_environment();
        env.add_template("test", "{{ table.row(['Hello', 'World']) }}")
            .unwrap();

        let tmpl = env.get_template("test").unwrap();
        let output = tmpl
            .render(minijinja::context! { table => Value::from_object(formatter) })
            .unwrap();

        assert_eq!(output, "Hello      | World   ");
    }

    #[test]
    fn object_row_method_in_loop() {
        let spec = TabularSpec::builder()
            .column(Column::new(Width::Fixed(8)))
            .column(Column::new(Width::Fixed(6)))
            .separator("  ")
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let mut env = crate::template::new_environment();
        env.add_template(
            "test",
            "{% for item in items %}{{ table.row([item.name, item.value]) }}\n{% endfor %}",
        )
        .unwrap();

        let tmpl = env.get_template("test").unwrap();
        let output = tmpl
            .render(minijinja::context! {
                table => Value::from_object(formatter),
                items => vec![
                    minijinja::context! { name => "Alice", value => "100" },
                    minijinja::context! { name => "Bob", value => "200" },
                ]
            })
            .unwrap();

        assert!(output.contains("Alice"));
        assert!(output.contains("Bob"));
    }

    #[test]
    fn object_column_width_method_via_template() {
        let spec = TabularSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(8)))
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let mut env = crate::template::new_environment();
        env.add_template(
            "test",
            "{{ table.column_width(0) }}-{{ table.column_width(1) }}",
        )
        .unwrap();

        let tmpl = env.get_template("test").unwrap();
        let output = tmpl
            .render(minijinja::context! { table => Value::from_object(formatter) })
            .unwrap();

        assert_eq!(output, "10-8");
    }

    #[test]
    fn object_attribute_access_via_template() {
        let spec = TabularSpec::builder()
            .column(Column::new(Width::Fixed(10)))
            .column(Column::new(Width::Fixed(8)))
            .separator(" | ")
            .build();
        let formatter = TabularFormatter::new(&spec, 80);

        let mut env = crate::template::new_environment();
        env.add_template(
            "test",
            "cols={{ table.num_columns }}, sep='{{ table.separator }}'",
        )
        .unwrap();

        let tmpl = env.get_template("test").unwrap();
        let output = tmpl
            .render(minijinja::context! { table => Value::from_object(formatter) })
            .unwrap();

        assert_eq!(output, "cols=2, sep=' | '");
    }
}
