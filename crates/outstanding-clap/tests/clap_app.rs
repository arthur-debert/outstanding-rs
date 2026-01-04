

use clap::{error::ErrorKind, Command};
use outstanding_clap::render_help;

#[test]
fn test_clap_integration_flow() {
    // 1. Setup a standard clap app
    let cmd = Command::new("myapp").about("My cool app").version("1.0");

    // 2. Simulate a help request (e.g., user passed --help)
    let res = cmd.clone().try_get_matches_from(vec!["myapp", "--help"]);

    // 3. Verify we catch the right error kind
    if let Err(e) = res {
        assert_eq!(e.kind(), ErrorKind::DisplayHelp);
        // 4. Render our custom help
        let help = render_help(&cmd, None).unwrap();

        // 5. Verify minimal content expectations
        assert!(help.contains("My cool app"));
        assert!(help.contains("Usage:"));
        // Check for styling ANSI if color is auto-detected (might be disabled in test env)
        // We can force force_color in config if we want to check ANSI.
    } else {
        panic!("Expected help error");
    }
}
