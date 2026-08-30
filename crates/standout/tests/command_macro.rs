#![allow(non_snake_case)] // Generated handler names use __handler suffix

use serde::Serialize;
use serde_json::json;
use standout::cli::handler::{CommandContext, Output};
use standout::command;
use standout_dispatch::verify::verify_handler_args;

#[derive(Serialize)]
struct ListOutput {
    all: bool,
}

#[command(name = "list", about = "List all items")]
fn list_cmd(
    #[flag(short = 'a', help = "Show all items")] all: bool,
) -> Result<Output<ListOutput>, anyhow::Error> {
    Ok(Output::Render(ListOutput { all }))
}

#[test]
fn test_command_generates_handler() {
    let cmd = list_cmd__command();
    let matches = cmd.try_get_matches_from(["list", "-a"]).unwrap();

    let ctx = CommandContext::default();
    let result = list_cmd__handler(&matches, &ctx);
    assert!(result.is_ok());
}

#[test]
fn test_command_generates_clap_command() {
    let cmd = list_cmd__command();
    assert_eq!(cmd.get_name(), "list");

    let arg = cmd.get_arguments().find(|a| a.get_id() == "all");
    assert!(arg.is_some());

    let arg = arg.unwrap();
    assert_eq!(arg.get_short(), Some('a'));
}

#[test]
fn test_command_generates_template() {
    assert_eq!(list_cmd__template(), "list");
}

#[test]
fn test_command_generates_expected_args() {
    let expected = list_cmd__expected_args();
    assert_eq!(expected.len(), 1);
    assert_eq!(expected[0].cli_name, "all");
}

#[test]
fn test_command_handler_struct() {
    use standout_dispatch::Handler;

    let mut handler = list_cmd_Handler;
    let cmd = list_cmd__command();
    let matches = cmd.try_get_matches_from(["list"]).unwrap();
    let ctx = CommandContext::default();

    let result = handler.handle(&matches, &ctx);
    assert!(result.is_ok());

    let expected = handler.expected_args();
    assert_eq!(expected.len(), 1);
}

#[test]
fn test_command_verification_passes() {
    let cmd = list_cmd__command();
    let expected = list_cmd__expected_args();
    assert!(verify_handler_args(&cmd, "list_cmd", &expected).is_ok());
}

#[command(name = "add", about = "Add an item", template = "add_item_view")]
fn add_cmd(#[arg(help = "Item name")] name: String) -> Result<Output<String>, anyhow::Error> {
    Ok(Output::Render(name))
}

#[test]
fn test_custom_template() {
    assert_eq!(add_cmd__template(), "add_item_view");
}

#[test]
fn test_required_arg() {
    let cmd = add_cmd__command();
    let arg = cmd.get_arguments().find(|a| a.get_id() == "name").unwrap();
    assert!(arg.is_required_set());
}

#[command(name = "search")]
fn search_cmd(
    #[arg(short = 'q', long = "query", help = "Search query")] query: Option<String>,
) -> Result<Output<Option<String>>, anyhow::Error> {
    Ok(Output::Render(query))
}

#[test]
fn test_optional_arg_present() {
    let cmd = search_cmd__command();
    let matches = cmd.try_get_matches_from(["search", "-q", "test"]).unwrap();

    let ctx = CommandContext::default();
    let result = search_cmd__handler(&matches, &ctx);
    assert!(result.is_ok());
}

#[test]
fn test_optional_arg_missing() {
    let cmd = search_cmd__command();
    let matches = cmd.try_get_matches_from(["search"]).unwrap();

    let ctx = CommandContext::default();
    let result = search_cmd__handler(&matches, &ctx);
    assert!(result.is_ok());
}

#[command(name = "process", about = "Process items")]
fn process_cmd(
    #[flag(short = 'v', long = "verbose")] verbose: bool,
    #[flag(short = 'd', long = "dry-run", help = "Dry run mode")] dry_run: bool,
    #[arg(short = 'l', long = "limit")] limit: Option<String>,
) -> Result<Output<serde_json::Value>, anyhow::Error> {
    Ok(Output::Render(json!({
        "verbose": verbose,
        "dry_run": dry_run,
        "limit": limit,
    })))
}

#[test]
fn test_multiple_params() {
    let cmd = process_cmd__command();

    assert!(cmd.get_arguments().any(|a| a.get_id() == "verbose"));
    assert!(cmd.get_arguments().any(|a| a.get_id() == "dry-run"));
    assert!(cmd.get_arguments().any(|a| a.get_id() == "limit"));

    let matches = cmd
        .try_get_matches_from(["process", "-v", "--dry-run", "-l", "10"])
        .unwrap();

    let ctx = CommandContext::default();
    let result = process_cmd__handler(&matches, &ctx);
    assert!(result.is_ok());
}

#[test]
fn test_expected_args_for_multiple_params() {
    let expected = process_cmd__expected_args();
    assert_eq!(expected.len(), 3);

    assert!(expected.iter().any(|e| e.cli_name == "verbose"));
    assert!(expected.iter().any(|e| e.cli_name == "dry-run"));
    assert!(expected.iter().any(|e| e.cli_name == "limit"));
}

#[command(name = "open")]
fn open_cmd(
    #[arg(positional, help = "File path")] path: String,
) -> Result<Output<String>, anyhow::Error> {
    Ok(Output::Render(path))
}

#[test]
fn test_positional_arg() {
    let cmd = open_cmd__command();
    let matches = cmd.try_get_matches_from(["open", "/path/to/file"]).unwrap();

    let ctx = CommandContext::default();
    let result = open_cmd__handler(&matches, &ctx);
    assert!(result.is_ok());
}

#[command(name = "info")]
fn info_cmd(#[ctx] ctx: &CommandContext) -> Result<Output<usize>, anyhow::Error> {
    Ok(Output::Render(ctx.command_path.len()))
}

#[test]
fn test_ctx_access() {
    let cmd = info_cmd__command();
    let matches = cmd.try_get_matches_from(["info"]).unwrap();

    let ctx = CommandContext {
        command_path: vec!["app".to_string(), "info".to_string()],
        ..Default::default()
    };

    let result = info_cmd__handler(&matches, &ctx);
    assert!(result.is_ok());
}

#[test]
fn test_ctx_not_in_expected_args() {
    let expected = info_cmd__expected_args();
    assert!(expected.is_empty());
}

#[command(name = "tag")]
fn tag_cmd(
    #[arg(short = 't', long = "tag", help = "Tags to apply")] tags: Vec<String>,
) -> Result<Output<Vec<String>>, anyhow::Error> {
    Ok(Output::Render(tags))
}

#[test]
fn test_vec_arg() {
    let cmd = tag_cmd__command();
    let matches = cmd
        .try_get_matches_from(["tag", "-t", "foo", "-t", "bar"])
        .unwrap();

    let ctx = CommandContext::default();
    let result = tag_cmd__handler(&matches, &ctx);
    assert!(result.is_ok());
}

#[test]
fn test_original_function_preserved() {
    let result = list_cmd(true);
    assert!(result.is_ok());

    let result = add_cmd("test".to_string());
    assert!(result.is_ok());
}

#[command(name = "paginate")]
fn paginate_cmd(
    #[arg(long = "page-size", default = "20", help = "Items per page")] page_size: String,
) -> Result<Output<String>, anyhow::Error> {
    Ok(Output::Render(page_size))
}

#[test]
fn test_default_value() {
    let cmd = paginate_cmd__command();

    let matches = cmd.clone().try_get_matches_from(["paginate"]).unwrap();
    let ctx = CommandContext::default();
    let result = paginate_cmd__handler(&matches, &ctx);
    assert!(result.is_ok());

    let matches = cmd
        .try_get_matches_from(["paginate", "--page-size", "50"])
        .unwrap();
    let result = paginate_cmd__handler(&matches, &ctx);
    assert!(result.is_ok());
}

#[derive(Serialize)]
struct TypedOutput {
    limit: usize,
    offset: Option<i32>,
    values: Vec<u64>,
}

#[command(name = "typed", about = "Command with typed args")]
fn typed_cmd(
    #[arg(short = 'l', long = "limit", help = "Result limit")] limit: usize,
    #[arg(short = 'o', long = "offset", help = "Result offset")] offset: Option<i32>,
    #[arg(short = 'v', long = "value", help = "Values to process")] values: Vec<u64>,
) -> Result<Output<TypedOutput>, anyhow::Error> {
    Ok(Output::Render(TypedOutput {
        limit,
        offset,
        values,
    }))
}

#[test]
fn test_typed_required_usize() {
    let cmd = typed_cmd__command();
    let matches = cmd.try_get_matches_from(["typed", "-l", "42"]).unwrap();

    let ctx = CommandContext::default();
    let result = typed_cmd__handler(&matches, &ctx);
    assert!(result.is_ok());
}

#[test]
fn test_typed_optional_i32() {
    let cmd = typed_cmd__command();

    let matches = cmd
        .clone()
        .try_get_matches_from(["typed", "-l", "10"])
        .unwrap();
    let ctx = CommandContext::default();
    let result = typed_cmd__handler(&matches, &ctx);
    assert!(result.is_ok());

    let matches = cmd
        .try_get_matches_from(["typed", "-l", "10", "-o", "5"])
        .unwrap();
    let result = typed_cmd__handler(&matches, &ctx);
    assert!(result.is_ok());
}

#[test]
fn test_typed_vec_u64() {
    let cmd = typed_cmd__command();
    let matches = cmd
        .try_get_matches_from(["typed", "-l", "10", "-v", "100", "-v", "200", "-v", "300"])
        .unwrap();

    let ctx = CommandContext::default();
    let result = typed_cmd__handler(&matches, &ctx);
    assert!(result.is_ok());
}

#[test]
fn test_typed_invalid_value_rejected() {
    let cmd = typed_cmd__command();

    let result = cmd.try_get_matches_from(["typed", "-l", "abc"]);
    assert!(result.is_err());
}

#[test]
fn test_typed_verification_passes() {
    let cmd = typed_cmd__command();
    let expected = typed_cmd__expected_args();
    assert!(verify_handler_args(&cmd, "typed_cmd", &expected).is_ok());
}

use std::path::PathBuf;

#[command(name = "read-file")]
fn read_file_cmd(
    #[arg(positional, help = "Path to file")] path: PathBuf,
) -> Result<Output<String>, anyhow::Error> {
    Ok(Output::Render(path.display().to_string()))
}

#[test]
fn test_pathbuf_arg() {
    let cmd = read_file_cmd__command();
    let matches = cmd
        .try_get_matches_from(["read-file", "/some/path/to/file.txt"])
        .unwrap();

    let ctx = CommandContext::default();
    let result = read_file_cmd__handler(&matches, &ctx);
    assert!(result.is_ok());
}

#[derive(Serialize)]
struct FloatOutput {
    factor: f64,
    precision: Option<f32>,
}

#[command(name = "scale")]
fn scale_cmd(
    #[arg(short = 'f', long = "factor", help = "Scale factor")] factor: f64,
    #[arg(short = 'p', long = "precision", help = "Precision")] precision: Option<f32>,
) -> Result<Output<FloatOutput>, anyhow::Error> {
    Ok(Output::Render(FloatOutput { factor, precision }))
}

#[test]
fn test_float_args() {
    let cmd = scale_cmd__command();
    let matches = cmd
        .try_get_matches_from(["scale", "-f", "2.5", "-p", "0.001"])
        .unwrap();

    let ctx = CommandContext::default();
    let result = scale_cmd__handler(&matches, &ctx);
    assert!(result.is_ok());
}

#[test]
fn test_float_scientific_notation() {
    let cmd = scale_cmd__command();
    let matches = cmd.try_get_matches_from(["scale", "-f", "1e-6"]).unwrap();

    let ctx = CommandContext::default();
    let result = scale_cmd__handler(&matches, &ctx);
    assert!(result.is_ok());
}

#[command(name = "offset")]
fn offset_cmd(
    #[arg(short = 'x', allow_negative_numbers, help = "X offset")] x: i32,
    #[arg(short = 'y', allow_negative_numbers, help = "Y offset")] y: i32,
) -> Result<Output<(i32, i32)>, anyhow::Error> {
    Ok(Output::Render((x, y)))
}

#[test]
fn test_negative_numbers_allowed() {
    let cmd = offset_cmd__command();
    let matches = cmd
        .try_get_matches_from(["offset", "-x", "-10", "-y", "5"])
        .unwrap();

    let ctx = CommandContext::default();
    let result = offset_cmd__handler(&matches, &ctx);
    assert!(result.is_ok());
}
