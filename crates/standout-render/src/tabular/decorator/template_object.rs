use super::*;

impl minijinja::value::Object for Table {
    fn get_value(self: &std::sync::Arc<Self>, key: &minijinja::Value) -> Option<minijinja::Value> {
        match key.as_str()? {
            "num_columns" => Some(minijinja::Value::from(self.num_columns())),
            "border" => Some(minijinja::Value::from(format!("{:?}", self.get_border()))),
            _ => None,
        }
    }

    fn enumerate(self: &std::sync::Arc<Self>) -> minijinja::value::Enumerator {
        minijinja::value::Enumerator::Str(&["num_columns", "border"])
    }

    fn call_method(
        self: &std::sync::Arc<Self>,
        _state: &minijinja::State,
        name: &str,
        args: &[minijinja::Value],
    ) -> Result<minijinja::Value, minijinja::Error> {
        match name {
            "row" => {
                if args.is_empty() {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::MissingArgument,
                        "row() requires an array of values",
                    ));
                }

                let values_arg = &args[0];

                if self.formatter.has_sub_columns() {
                    let outer_iter = match values_arg.try_iter() {
                        Ok(iter) => iter,
                        Err(_) => {
                            let values = vec![stringify(values_arg).into_owned()];
                            return Ok(fragment(self.row_markup(&values)));
                        }
                    };

                    let mut owned_values: Vec<OwnedCellValue> = Vec::new();
                    for (i, v) in outer_iter.enumerate() {
                        let is_sub_col = self
                            .formatter
                            .columns()
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

                    let formatted = self.row_markup_cells(&cell_values);
                    Ok(fragment(formatted))
                } else {
                    let values: Vec<String> = match values_arg.try_iter() {
                        Ok(iter) => iter.map(|v| stringify(&v).into_owned()).collect(),
                        Err(_) => vec![stringify(values_arg).into_owned()],
                    };

                    let formatted = self.row_markup(&values);
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
                let formatted = self.formatter.row_from(&value);
                Ok(fragment(self.wrap_data_row(&formatted)))
            }
            "header_row" => Ok(fragment(self.header_row())),
            "separator_row" => Ok(fragment(self.separator_row())),
            "top_border" => Ok(fragment(self.top_border())),
            "bottom_border" => Ok(fragment(self.bottom_border())),
            "render_all" => {
                if args.is_empty() {
                    return Err(minijinja::Error::new(
                        minijinja::ErrorKind::MissingArgument,
                        "render_all() requires an array of rows",
                    ));
                }

                let rows_iter = args[0].try_iter().map_err(|_| {
                    minijinja::Error::new(
                        minijinja::ErrorKind::InvalidOperation,
                        "render_all() requires an array of rows",
                    )
                })?;

                let rows: Vec<Vec<String>> = rows_iter
                    .map(|row| {
                        row.try_iter()
                            .map(|iter| iter.map(|v| stringify(&v).into_owned()).collect())
                            .unwrap_or_else(|_| vec![stringify(&row).into_owned()])
                    })
                    .collect();

                let formatted = self.render_markup(&rows);
                Ok(fragment(formatted))
            }
            _ => Err(minijinja::Error::new(
                minijinja::ErrorKind::UnknownMethod,
                format!("Table has no method '{}'", name),
            )),
        }
    }
}
