use clap::Command;
use outstanding::topics::{Topic, TopicRegistry, TopicType};
use outstanding_clap::{display_with_pager, HelpResult, Outstanding};

fn setup_registry() -> TopicRegistry {
    let mut registry = TopicRegistry::new();

    // Add a normal topic
    registry.add_topic(Topic::new(
        "guidelines",
        "Follow these guidelines...",
        TopicType::Text,
        None,
    ));

    // Add a topic that collides with a command name to test priority
    registry.add_topic(Topic::new(
        "init",
        "This topic is named init but command should shadow it",
        TopicType::Text,
        None,
    ));

    registry
}

fn setup_command() -> Command {
    Command::new("myapp")
        .subcommand(Command::new("init").about("Initialize the app"))
        .subcommand(Command::new("run").about("Run the app"))
}

#[test]
fn test_help_topic_resolution() {
    let registry = setup_registry();
    let outstanding = Outstanding::with_registry(registry);
    let cmd = setup_command();

    // 1. "help guidelines" -> Should find the topic
    let res = outstanding.get_matches_from(cmd.clone(), vec!["myapp", "help", "guidelines"]);

    if let HelpResult::Help(h) = res {
        assert!(h.contains("Follow these guidelines..."));
    } else {
        panic!("Expected Help for topic, got {:?}", res);
    }
}

#[test]
fn test_help_command_shadows_topic() {
    let registry = setup_registry();
    let outstanding = Outstanding::with_registry(registry);
    let cmd = setup_command();

    // 2. "help init" -> Should find command help, NOT topic content
    let res = outstanding.get_matches_from(cmd.clone(), vec!["myapp", "help", "init"]);

    if let HelpResult::Help(h) = res {
        // It should be clap help for init command
        assert!(h.contains("Initialize the app"));
        // It should NOT contain the topic content
        assert!(!h.contains("This topic is named init but command should shadow it"));
    } else {
        panic!("Expected Help for command, got {:?}", res);
    }
}

#[test]
fn test_help_unknown() {
    let registry = setup_registry();
    let outstanding = Outstanding::with_registry(registry);
    let cmd = setup_command();

    // 3. "help whatever" -> Should be Error
    let res = outstanding.get_matches_from(cmd.clone(), vec!["myapp", "help", "whatever"]);

    if let HelpResult::Error(e) = res {
        assert_eq!(e.kind(), clap::error::ErrorKind::InvalidSubcommand);
    } else {
        panic!("Expected Error for unknown topic, got {:?}", res);
    }
}

#[test]
fn test_normal_execution() {
    let registry = setup_registry();
    let outstanding = Outstanding::with_registry(registry);
    let cmd = setup_command();

    // 4. "run" -> Should be normal matches
    let res = outstanding.get_matches_from(cmd.clone(), vec!["myapp", "run"]);

    if let HelpResult::Matches(m) = res {
        assert_eq!(m.subcommand_name(), Some("run"));
    } else {
        panic!("Expected Matches for normal command, got {:?}", res);
    }
}

#[test]
fn test_help_without_args() {
    let registry = setup_registry();
    let outstanding = Outstanding::with_registry(registry);
    let cmd = setup_command();

    // "help" without args -> Should return root help
    let res = outstanding.get_matches_from(cmd.clone(), vec!["myapp", "help"]);

    if let HelpResult::Help(h) = res {
        // Should contain info about the app
        assert!(h.contains("myapp"));
    } else {
        panic!("Expected Help for root help, got {:?}", res);
    }
}

#[test]
fn test_nested_subcommand_help() {
    let outstanding = Outstanding::new();

    // Create a command with nested subcommands
    let cmd = Command::new("myapp").subcommand(
        Command::new("config")
            .about("Configuration commands")
            .subcommand(Command::new("get").about("Get a config value"))
            .subcommand(Command::new("set").about("Set a config value")),
    );

    // "help config get" -> Should return help for nested subcommand
    let res = outstanding.get_matches_from(cmd.clone(), vec!["myapp", "help", "config", "get"]);

    if let HelpResult::Help(h) = res {
        assert!(h.contains("Get a config value"));
    } else {
        panic!("Expected Help for nested subcommand, got {:?}", res);
    }
}

#[test]
fn test_page_flag_with_topic() {
    let registry = setup_registry();
    let outstanding = Outstanding::with_registry(registry);
    let cmd = setup_command();

    // "help --page guidelines" -> Should return PagedHelp
    let res =
        outstanding.get_matches_from(cmd.clone(), vec!["myapp", "help", "--page", "guidelines"]);

    if let HelpResult::PagedHelp(h) = res {
        assert!(h.contains("Follow these guidelines..."));
    } else {
        panic!("Expected PagedHelp for topic with --page, got {:?}", res);
    }
}

#[test]
fn test_page_flag_with_command() {
    let registry = setup_registry();
    let outstanding = Outstanding::with_registry(registry);
    let cmd = setup_command();

    // "help --page init" -> Should return PagedHelp for command
    let res = outstanding.get_matches_from(cmd.clone(), vec!["myapp", "help", "--page", "init"]);

    if let HelpResult::PagedHelp(h) = res {
        assert!(h.contains("Initialize the app"));
    } else {
        panic!("Expected PagedHelp for command with --page, got {:?}", res);
    }
}

#[test]
fn test_page_flag_without_topic() {
    let registry = setup_registry();
    let outstanding = Outstanding::with_registry(registry);
    let cmd = setup_command();

    // "help --page" without topic -> Should return PagedHelp for root
    let res = outstanding.get_matches_from(cmd.clone(), vec!["myapp", "help", "--page"]);

    if let HelpResult::PagedHelp(h) = res {
        assert!(h.contains("myapp"));
    } else {
        panic!(
            "Expected PagedHelp for root help with --page, got {:?}",
            res
        );
    }
}

#[test]
fn test_page_flag_position_after_topic() {
    let registry = setup_registry();
    let outstanding = Outstanding::with_registry(registry);
    let cmd = setup_command();

    // "help guidelines --page" -> Should also work with flag after topic
    let res =
        outstanding.get_matches_from(cmd.clone(), vec!["myapp", "help", "guidelines", "--page"]);

    if let HelpResult::PagedHelp(h) = res {
        assert!(h.contains("Follow these guidelines..."));
    } else {
        panic!(
            "Expected PagedHelp for topic with --page after topic, got {:?}",
            res
        );
    }
}

#[test]
fn test_no_page_flag_returns_help() {
    let registry = setup_registry();
    let outstanding = Outstanding::with_registry(registry);
    let cmd = setup_command();

    // "help guidelines" without --page -> Should return Help (not PagedHelp)
    let res = outstanding.get_matches_from(cmd.clone(), vec!["myapp", "help", "guidelines"]);

    // Should be regular Help, not PagedHelp
    match res {
        HelpResult::Help(h) => {
            assert!(h.contains("Follow these guidelines..."));
        }
        HelpResult::PagedHelp(_) => {
            panic!("Expected Help without --page, got PagedHelp");
        }
        _ => {
            panic!("Expected Help for topic without --page, got {:?}", res);
        }
    }
}

#[test]
fn test_display_with_pager_import() {
    // Just verify that display_with_pager is exported and callable
    // We can't easily test actual pager behavior in unit tests
    // but we can verify the function exists and is public
    let _ = display_with_pager as fn(&str) -> std::io::Result<()>;
}

#[test]
fn test_help_topics_lists_all_topics() {
    let registry = setup_registry();
    let outstanding = Outstanding::with_registry(registry);
    let cmd = setup_command();

    // "help topics" -> Should list all available topics
    let res = outstanding.get_matches_from(cmd.clone(), vec!["myapp", "help", "topics"]);

    if let HelpResult::Help(h) = res {
        // Should contain "Available Topics" header
        assert!(
            h.contains("Available Topics"),
            "Should have 'Available Topics' header"
        );
        // Should list our topics
        assert!(h.contains("guidelines"), "Should list 'guidelines' topic");
        assert!(h.contains("init"), "Should list 'init' topic");
    } else {
        panic!("Expected Help for topics list, got {:?}", res);
    }
}

#[test]
fn test_help_topics_with_pager() {
    let registry = setup_registry();
    let outstanding = Outstanding::with_registry(registry);
    let cmd = setup_command();

    // "help --page topics" -> Should return PagedHelp with topics list
    let res = outstanding.get_matches_from(cmd.clone(), vec!["myapp", "help", "--page", "topics"]);

    if let HelpResult::PagedHelp(h) = res {
        assert!(h.contains("Available Topics"));
        assert!(h.contains("guidelines"));
    } else {
        panic!("Expected PagedHelp for topics with --page, got {:?}", res);
    }
}

#[test]
fn test_help_topics_empty_registry() {
    let outstanding = Outstanding::new(); // Empty registry
    let cmd = setup_command();

    // "help topics" with empty registry -> Should still work, just show empty list
    let res = outstanding.get_matches_from(cmd.clone(), vec!["myapp", "help", "topics"]);

    if let HelpResult::Help(h) = res {
        assert!(h.contains("Available Topics"));
        assert!(h.contains("TOPICS"));
    } else {
        panic!("Expected Help for empty topics list, got {:?}", res);
    }
}

#[test]
fn test_root_help_shows_learn_more_section() {
    let registry = setup_registry();
    let outstanding = Outstanding::with_registry(registry);
    let cmd = setup_command();

    // "help" without args -> Should show root help with Learn More section
    let res = outstanding.get_matches_from(cmd.clone(), vec!["myapp", "help"]);

    if let HelpResult::Help(h) = res {
        // Should contain the Learn More section
        assert!(h.contains("LEARN MORE"), "Should have 'LEARN MORE' header");
        // Should list our topics
        assert!(
            h.contains("guidelines"),
            "Should list 'guidelines' topic in Learn More"
        );
        assert!(h.contains("init"), "Should list 'init' topic in Learn More");
    } else {
        panic!("Expected Help for root help, got {:?}", res);
    }
}

#[test]
fn test_root_help_no_learn_more_when_empty_registry() {
    let outstanding = Outstanding::new(); // Empty registry
    let cmd = setup_command();

    // "help" with empty registry -> Should NOT show Learn More section
    let res = outstanding.get_matches_from(cmd.clone(), vec!["myapp", "help"]);

    if let HelpResult::Help(h) = res {
        // Should NOT contain the Learn More section when there are no topics
        assert!(
            !h.contains("LEARN MORE"),
            "Should NOT have 'LEARN MORE' header when no topics"
        );
    } else {
        panic!("Expected Help for root help, got {:?}", res);
    }
}
