use minijinja::machinery::{ast, parse, tokenize, Span, Token};
use minijinja::{Error, ErrorKind};

pub(crate) fn prepare(source: &str) -> Result<String, Error> {
    let tree = parse(source, "<template>", Default::default(), Default::default())?;
    let tokens = tokenize(source, false, Default::default(), Default::default())
        .collect::<Result<Vec<_>, _>>()?;
    let mut adapter = Adapter {
        source,
        tokens,
        wrappers: Vec::new(),
        insertions: Vec::new(),
        replacements: Vec::new(),
    };
    adapter.statement(&tree)?;
    let prepared = adapter.finish()?;
    parse(
        &prepared,
        "<template>",
        Default::default(),
        Default::default(),
    )
    .map_err(|error| {
        Error::new(
            ErrorKind::InvalidOperation,
            format!("unsupported terminal template expression: {error}"),
        )
    })?;
    Ok(prepared)
}

pub(crate) fn validate_literal(source: &str) -> Result<(), Error> {
    let mut bracket = false;
    let mut escaped = false;
    for character in source.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '[' => bracket = true,
            ']' | '\n' => bracket = false,
            _ => {}
        }
    }
    if bracket || escaped {
        return Err(Error::new(ErrorKind::InvalidOperation,
            "terminal template literal fragments must end with complete tags and backslash escapes; escape literal brackets/backslashes, and use value|style_as(name) for dynamic styles"));
    }
    Ok(())
}

struct Wrapper {
    start: usize,
    end: usize,
    filter: &'static str,
}

struct Adapter<'a> {
    source: &'a str,
    tokens: Vec<(Token<'a>, Span)>,
    wrappers: Vec<Wrapper>,
    insertions: Vec<(usize, String)>,
    replacements: Vec<(usize, usize, String)>,
}

impl Adapter<'_> {
    fn statements(&mut self, statements: &[ast::Stmt<'_>]) -> Result<(), Error> {
        statements.iter().try_for_each(|stmt| self.statement(stmt))
    }

    fn statement(&mut self, statement: &ast::Stmt<'_>) -> Result<(), Error> {
        match statement {
            ast::Stmt::Template(node) => self.statements(&node.children)?,
            ast::Stmt::EmitExpr(node) => self.expression(&node.expr),
            ast::Stmt::EmitRaw(node) => validate_literal(node.raw)?,
            ast::Stmt::ForLoop(node) => {
                self.expression(&node.iter);
                self.optional_expression(&node.filter_expr);
                self.statements(&node.body)?;
                self.statements(&node.else_body)?;
            }
            ast::Stmt::IfCond(node) => {
                self.expression(&node.expr);
                self.statements(&node.true_body)?;
                self.statements(&node.false_body)?;
            }
            ast::Stmt::WithBlock(node) => {
                for (_, value) in &node.assignments {
                    self.expression(value);
                }
                self.statements(&node.body)?;
            }
            ast::Stmt::Set(node) => self.expression(&node.expr),
            ast::Stmt::SetBlock(node) => {
                let target = assignment_name(&node.target)?;
                if let Some(filter) = &node.filter {
                    self.block_filter(filter)?;
                }
                self.statements(&node.body)?;
                let end = self
                    .tokens
                    .iter()
                    .find(|(token, span)| {
                        span.start_offset >= node.span().end_offset
                            && matches!(token, Token::BlockEnd)
                    })
                    .map(|(_, span)| *span)
                    .ok_or_else(unsupported_span)?;
                let delimiter = self
                    .source
                    .get(end.start_offset as usize..end.end_offset as usize)
                    .ok_or_else(unsupported_span)?;
                let closing = if delimiter.ends_with("-%}") {
                    "-%}"
                } else {
                    "%}"
                };
                self.insertions.push((
                    end.end_offset as usize,
                    format!("{{% set {target} = {target}|__standout_capture {closing}"),
                ));
            }
            ast::Stmt::AutoEscape(_) => {
                return Err(Error::new(
                    ErrorKind::InvalidOperation,
                    "terminal escaping cannot be disabled or changed; remove the autoescape block",
                ));
            }
            ast::Stmt::FilterBlock(node) => {
                self.block_filter(&node.filter)?;
                self.statements(&node.body)?;
            }
            ast::Stmt::Block(node) => self.statements(&node.body)?,
            ast::Stmt::Import(node) => self.expression(&node.expr),
            ast::Stmt::FromImport(node) => self.expression(&node.expr),
            ast::Stmt::Extends(node) => self.expression(&node.name),
            ast::Stmt::Include(node) => self.expression(&node.name),
            ast::Stmt::Macro(node) => self.macro_body(node)?,
            ast::Stmt::CallBlock(node) => {
                let start = expression_start(&node.call.expr) as usize;
                let end = node.call.expr.span().end_offset as usize;
                let callee = self.source.get(start..end).ok_or_else(unsupported_span)?;
                let opening = self
                    .tokens
                    .iter()
                    .find(|(token, span)| {
                        span.start_offset as usize >= end && matches!(token, Token::ParenOpen)
                    })
                    .map(|(_, span)| span.end_offset as usize)
                    .ok_or_else(unsupported_span)?;
                self.replacements
                    .push((start, end, "__standout_call".into()));
                self.insertions.push((opening, format!("{callee},")));
                self.arguments(&node.call.args);
                self.macro_body(&node.macro_decl)?;
            }
            ast::Stmt::Do(node) => self.call_children(&node.call),
            #[allow(unreachable_patterns)]
            _ => {
                return Err(Error::new(
                    ErrorKind::InvalidOperation,
                    "unsupported statement in terminal template",
                ));
            }
        }
        Ok(())
    }

    fn macro_body(&mut self, node: &ast::Macro<'_>) -> Result<(), Error> {
        for value in &node.defaults {
            self.expression(value);
        }
        self.statements(&node.body)
    }

    fn block_filter(&mut self, expression: &ast::Expr<'_>) -> Result<(), Error> {
        let mut first = expression;
        loop {
            match first {
                ast::Expr::Filter(node) => match &node.expr {
                    Some(input) => first = input,
                    None => {
                        self.insertions.push((
                            node.span().start_offset as usize,
                            "__standout_capture|".to_owned(),
                        ));
                        break;
                    }
                },
                _ => return Err(unsupported_span()),
            }
        }
        self.expression(expression);
        Ok(())
    }

    fn optional_expression(&mut self, expression: &Option<ast::Expr<'_>>) {
        if let Some(expression) = expression {
            self.expression(expression);
        }
    }

    fn call_children(&mut self, call: &ast::Call<'_>) {
        self.expression(&call.expr);
        self.arguments(&call.args);
    }

    fn arguments(&mut self, arguments: &[ast::CallArg<'_>]) {
        for argument in arguments {
            let (ast::CallArg::Pos(expr)
            | ast::CallArg::Kwarg(_, expr)
            | ast::CallArg::PosSplat(expr)
            | ast::CallArg::KwargSplat(expr)) = argument;
            self.expression(expr);
        }
    }

    fn expression(&mut self, expression: &ast::Expr<'_>) {
        match expression {
            ast::Expr::Var(_) | ast::Expr::Const(_) => {}
            ast::Expr::Slice(node) => {
                self.wrap(&node.expr, "__standout_plain_if_formatted");
                self.expression(&node.expr);
                self.optional_expression(&node.start);
                self.optional_expression(&node.stop);
                self.optional_expression(&node.step);
            }
            ast::Expr::UnaryOp(node) => self.expression(&node.expr),
            ast::Expr::BinOp(node) => {
                if matches!(
                    node.op,
                    ast::BinOpKind::Eq
                        | ast::BinOpKind::Ne
                        | ast::BinOpKind::Lt
                        | ast::BinOpKind::Lte
                        | ast::BinOpKind::Gt
                        | ast::BinOpKind::Gte
                        | ast::BinOpKind::In
                ) {
                    self.wrap(&node.left, "__standout_plain_for_comparison");
                    self.wrap(&node.right, "__standout_plain_for_comparison");
                }
                self.expression(&node.left);
                self.expression(&node.right);
            }
            ast::Expr::Compare(node) => {
                self.wrap(&node.expr, "__standout_plain_for_comparison");
                self.expression(&node.expr);
                for op in &node.ops {
                    self.wrap(&op.expr, "__standout_plain_for_comparison");
                    self.expression(&op.expr);
                }
            }
            ast::Expr::IfExpr(node) => {
                self.expression(&node.test_expr);
                self.expression(&node.true_expr);
                self.optional_expression(&node.false_expr);
            }
            ast::Expr::Filter(node) => {
                self.optional_expression(&node.expr);
                self.arguments(&node.args);
            }
            ast::Expr::Test(node) => {
                self.expression(&node.expr);
                self.arguments(&node.args);
            }
            ast::Expr::GetAttr(node) => self.expression(&node.expr),
            ast::Expr::GetItem(node) => {
                self.wrap(&node.expr, "__standout_plain_if_formatted");
                self.expression(&node.expr);
                self.wrap(&node.subscript_expr, "__standout_plain_if_formatted");
                self.expression(&node.subscript_expr);
            }
            ast::Expr::Call(node) => {
                self.wrap(expression, "__standout_capture");
                self.call_children(node);
            }
            ast::Expr::List(node) => {
                for item in &node.items {
                    self.expression(item);
                }
            }
            ast::Expr::Map(node) => {
                for key in &node.keys {
                    self.wrap(key, "__standout_plain_if_formatted");
                    self.expression(key);
                }
                for value in &node.values {
                    self.expression(value);
                }
            }
        }
    }

    fn wrap(&mut self, expression: &ast::Expr<'_>, filter: &'static str) {
        self.wrappers.push(Wrapper {
            start: expression_start(expression) as usize,
            end: expression.span().end_offset as usize,
            filter,
        });
    }

    fn finish(self) -> Result<String, Error> {
        let mut events = Vec::new();
        for (index, wrapper) in self.wrappers.iter().enumerate() {
            events.push((
                wrapper.start,
                2,
                usize::MAX - wrapper.end,
                index,
                "((".to_owned(),
                wrapper.start,
            ));
            events.push((
                wrapper.end,
                0,
                usize::MAX - wrapper.start,
                usize::MAX - index,
                format!(")|{})", wrapper.filter),
                wrapper.end,
            ));
        }
        for (index, (offset, value)) in self.insertions.into_iter().enumerate() {
            events.push((offset, 1, 0, index, value, offset));
        }
        for (index, (start, end, value)) in self.replacements.into_iter().enumerate() {
            events.push((start, 3, 0, index, value, end));
        }
        events.sort_by(|left, right| {
            (left.0, left.1, left.2, left.3).cmp(&(right.0, right.1, right.2, right.3))
        });
        let mut result = String::with_capacity(self.source.len());
        let mut position = 0;
        for (offset, _, _, _, insertion, next_position) in events {
            result.push_str(
                self.source
                    .get(position..offset)
                    .ok_or_else(unsupported_span)?,
            );
            result.push_str(&insertion);
            position = next_position;
        }
        result.push_str(self.source.get(position..).ok_or_else(unsupported_span)?);
        Ok(result)
    }
}

fn expression_start(expression: &ast::Expr<'_>) -> u32 {
    let child = match expression {
        ast::Expr::Slice(node) => Some(&node.expr),
        ast::Expr::UnaryOp(node) => Some(&node.expr),
        ast::Expr::BinOp(node) => Some(&node.left),
        ast::Expr::Compare(node) => Some(&node.expr),
        ast::Expr::IfExpr(node) => Some(&node.true_expr),
        ast::Expr::Filter(node) => node.expr.as_ref(),
        ast::Expr::Test(node) => Some(&node.expr),
        ast::Expr::GetAttr(node) => Some(&node.expr),
        ast::Expr::GetItem(node) => Some(&node.expr),
        ast::Expr::Call(node) => Some(&node.expr),
        ast::Expr::Var(_) | ast::Expr::Const(_) | ast::Expr::List(_) | ast::Expr::Map(_) => None,
    };
    child.map_or(expression.span().start_offset, |child| {
        expression.span().start_offset.min(expression_start(child))
    })
}

fn assignment_name(target: &ast::Expr<'_>) -> Result<String, Error> {
    match target {
        ast::Expr::Var(node) => Ok(node.id.to_owned()),
        ast::Expr::GetAttr(node) => Ok(format!("{}.{}", assignment_name(&node.expr)?, node.name)),
        _ => Err(Error::new(
            ErrorKind::InvalidOperation,
            "terminal set captures require one variable or namespace attribute",
        )),
    }
}

fn unsupported_span() -> Error {
    Error::new(
        ErrorKind::InvalidOperation,
        "unsupported source span in terminal template",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use minijinja::{context, value::Value, Environment};

    fn environment() -> Environment<'static> {
        let mut environment = crate::template::spelling::new_environment();
        environment.set_formatter(minijinja::escape_formatter);
        environment.set_auto_escape_callback(|_| minijinja::AutoEscape::None);
        environment.add_filter("__standout_capture", |value: Value| value);
        environment.add_function(
            "__standout_call",
            |state: &minijinja::State, value: Value, args: minijinja::value::Rest<Value>| {
                value.call(state, &args)
            },
        );
        environment.add_filter("__standout_plain_if_formatted", |value: Value| value);
        environment.add_filter("__standout_plain_for_comparison", |value: Value| value);
        environment
    }

    fn assert_equivalent(source: &str) {
        let prepared = prepare(source).unwrap();
        let env = environment();
        let expected = env.render_str(source, context!()).unwrap();
        let actual = env.render_str(&prepared, context!()).unwrap();
        assert_eq!(actual, expected, "{prepared}");
        assert_eq!(source.matches('\n').count(), prepared.matches('\n').count());
    }

    #[test]
    fn wraps_nested_calls_and_keyword_arguments() {
        let source = "{{ outer(inner(), key=other(), *args(), **kwargs()) }}";
        let prepared = prepare(source).unwrap();
        assert_eq!(prepared.matches("|__standout_capture").count(), 5);
        assert!(prepared.contains("key=((other())|__standout_capture)"));
    }

    #[test]
    fn handles_postfix_spans_and_parentheses() {
        for expression in [
            "f()()",
            "obj.method()",
            "obj.first().second()",
            "(f())()",
            "(f() or g())()",
            "f()[g()]()",
            "(f()|default(g()))()",
            "f()[:g()]",
            "(f() ~ g())[1:]",
            "f()[:][1:]",
            "obj.method()[1:]",
            "[f(), g()][1:]",
            "(f() if g() else h())[:]",
        ] {
            let source = format!("{{{{ {expression} }}}}");
            prepare(&source).unwrap_or_else(|error| panic!("{source}: {error}"));
        }
    }

    #[test]
    fn rewritten_calls_match_original_execution() {
        let mut env = environment();
        env.add_function("f", || Value::from_function(|| "abcdef"));
        env.add_function("text", || "abcdef");
        env.add_function("number", || 2);
        env.add_function("args", || vec![2]);
        env.add_function("kwargs", || context!(b => 3));
        env.add_function("combine", |a: i64, kwargs: minijinja::value::Kwargs| {
            let b: i64 = kwargs.get("b")?;
            kwargs.assert_all_used()?;
            Ok::<_, Error>(a + b)
        });
        env.add_global(
            "obj",
            context!(
                method => Value::from_function(|| "abcdef"),
                first => Value::from_function(|| context!(
                    second => Value::from_function(|| "abcdef")
                ))
            ),
        );
        for expression in [
            "f()()",
            "obj.method()",
            "obj.first().second()",
            "(f())()",
            "(f() or f())()",
            "[f][0]()()",
            "(f()|default(f()))()",
            "text()[:number()]",
            "(text() ~ text())[1:]",
            "text()[:][1:]",
            "obj.method()[1:]",
            "[text(), text()][1:]",
            "(text() if number() else text())[:]",
            "combine(number(), b=number())",
            "combine(*args(), **kwargs())",
            "combine(number(), b=combine(number(), b=number()))",
        ] {
            let source = format!("{{{{ {expression} }}}}");
            let prepared = prepare(&source).unwrap();
            let expected = env.render_str(&source, context!()).unwrap();
            assert_eq!(
                env.render_str(&prepared, context!()).unwrap(),
                expected,
                "{prepared}"
            );
        }
    }

    #[test]
    fn preserves_macro_set_scopes_and_filters() {
        assert_equivalent(concat!(
            "{% macro echo(x='hi') %}{{ x }}{% endmacro %}\n",
            "{% with value=echo() %}{% set a %}{{ echo(value) }}{% endset %}",
            "{% set b | replace('h', 'H') %}{{ a }}{% endset %}{{ b }}{% endwith %}",
            "{% for x in [1, 2] %}{% set a %}{{ echo(x) }}{% endset %}{{ a }}{% endfor %}",
            "{% set ns = namespace() %}{% set ns.value %}ok{% endset %}{{ ns.value }}",
            "{% filter replace('a', 'A') %}abc{% endfilter %}",
        ));
    }

    #[test]
    fn preserves_nested_set_blocks_and_whitespace_controls() {
        for ending in ["%}", "-%}", "+%}"] {
            assert_equivalent(&format!(
                "before\n{{% set outer %}}\n{{% set inner %}}é{{% endset {ending}\n{{{{ inner }}}}{{% endset {ending}\n{{{{ outer }}}}\nafter\n"
            ));
        }
    }

    #[test]
    fn traverses_call_and_do_arguments_without_changing_statement_grammar() {
        assert_equivalent(concat!(
            "{% macro echo(x) %}{{ x }}{% endmacro %}",
            "{% macro wrap(x) %}{{ x }}{{ caller() }}{% endmacro %}",
            "{% call wrap(echo('a')) %}{{ echo('b') }}{% endcall %}",
            "{% do echo(echo('c')) %}",
        ));
    }

    #[test]
    fn leaves_raw_and_comments_untouched() {
        let source = "{# {{ call() }} {% autoescape false %} #}\n{% raw %}{{ f() }}{% autoescape true %}{% endraw %}\n";
        assert_eq!(prepare(source).unwrap(), source);
    }

    #[test]
    fn rejects_every_autoescape_block() {
        for setting in ["true", "false", "'html'", "'none'", "enabled()"] {
            let source = format!(
                "{{% macro unused() %}}{{% autoescape {setting} %}}x{{% endautoescape %}}{{% endmacro %}}"
            );
            let error = prepare(&source).unwrap_err();
            assert!(error
                .to_string()
                .contains("terminal escaping cannot be disabled"));
        }
    }

    #[test]
    fn preserves_original_syntax_error_location() {
        let source = "{{ f() }}\n{% set x = %}";
        let original = parse(source, "<template>", Default::default(), Default::default())
            .err()
            .unwrap();
        let adapted = prepare(source).unwrap_err();
        assert_eq!(adapted.kind(), original.kind());
        assert_eq!(adapted.line(), original.line());
        assert_eq!(adapted.to_string(), original.to_string());
    }

    #[test]
    fn slicing_preserves_sequence_and_string_behavior() {
        assert_equivalent("{{ [1, 2, 3][1:] }}|{{ ('abcd' ~ 'ef')[1:4] }}");
        assert_eq!(
            prepare("{{ value[:] }}").unwrap(),
            "{{ ((value)|__standout_plain_if_formatted)[:] }}"
        );
    }

    #[test]
    fn captures_before_set_uses_and_block_filters() {
        let mut env = environment();
        env.set_auto_escape_callback(|_| minijinja::AutoEscape::Html);
        env.add_filter("__standout_capture", |value: Value| {
            if value.is_safe() {
                Value::from("captured")
            } else {
                value
            }
        });
        for source in [
            "{% set x %}original{% endset %}{{ x|replace('captured', 'yes') }}",
            "{% set x | replace('captured', 'yes') %}original{% endset %}{{ x }}",
            "{% filter replace('captured', 'yes') %}original{% endfilter %}",
            "{% macro x() %}original{% endmacro %}{{ x()|replace('captured', 'yes') }}",
        ] {
            let prepared = prepare(source).unwrap();
            assert_eq!(
                env.render_str(&prepared, context!()).unwrap(),
                "yes",
                "{prepared}"
            );
        }
    }

    #[test]
    fn traverses_expression_and_template_statement_positions() {
        let source = concat!(
            "{% extends base() %}{% import module() as mod %}",
            "{% from module() import name as alias %}{% include partial() %}",
            "{% block body %}{% if check() %}{{ -number() }}",
            "{% elif other() %}{{ [one(), {'key': two()}] }}{% endif %}",
            "{% for item in items() if allowed(item) %}{{ item[get_key()] }}",
            "{% else %}{{ fallback() }}{% endfor %}",
            "{% set result = left() < middle() < right() %}",
            "{{ value() is equalto(expected()) }}",
            "{{ yes() if test() else no() }}{% endblock %}",
            "{% macro item(default=factory()) %}{{ default }}{% endmacro %}",
        );
        assert_eq!(
            prepare(source)
                .unwrap()
                .matches("|__standout_capture")
                .count(),
            22
        );
    }

    #[test]
    fn rejects_destructured_set_captures() {
        assert!(prepare("{% set a, b %}ab{% endset %}").is_err());
    }
}
