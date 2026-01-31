use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;
use thiserror::Error;
use wait_timeout::ChildExt;

#[derive(Debug, Error)]
pub enum ShellError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Command `{0}` timed out after {1:?}")]
    Timeout(String, Duration),
    #[error("Command `{0}` failed with status {1}")]
    CommandFailed(String, std::process::ExitStatus),
    #[error("Command output was not valid UTF-8")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
}

/// Execute a shell command with the given input piped to stdin.
///
/// Returns the command's stdout on success.
///
/// # Arguments
///
/// * `command_str` - The shell command to execute
/// * `input` - Data to write to the command's stdin
/// * `timeout` - Optional timeout; if exceeded, the process is killed
///
/// # Notes
///
/// The entire stdout is buffered in memory before being returned.
/// For very large outputs (multi-megabyte), consider streaming alternatives.
pub fn run_piped(
    command_str: &str,
    input: &str,
    timeout: Option<Duration>,
) -> Result<String, ShellError> {
    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(command_str);
        c
    } else {
        let mut c = Command::new("sh");
        c.arg("-c").arg(command_str);
        c
    };

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    let mut child = cmd.spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes())?;
    }

    match timeout {
        Some(duration) => match child.wait_timeout(duration)? {
            Some(status) => {
                if !status.success() {
                    return Err(ShellError::CommandFailed(
                        command_str.to_string(),
                        status,
                    ));
                }
            }
            None => {
                child.kill()?;
                return Err(ShellError::Timeout(command_str.to_string(), duration));
            }
        },
        None => {
            let status = child.wait()?;
            if !status.success() {
                return Err(ShellError::CommandFailed(
                    command_str.to_string(),
                    status,
                ));
            }
        }
    }

    let mut output = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        use std::io::Read;
        stdout.read_to_string(&mut output)?;
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_echo() {
        let cmd = if cfg!(windows) {
            "echo hello"
        } else {
            "echo hello"
        };
        let output = run_piped(cmd, "", None).unwrap();
        assert!(output.trim().contains("hello"));
    }

    #[test]
    fn test_input_piping() {
        let cmd = if cfg!(windows) {
            "findstr foo"
        } else {
            "grep foo"
        };
        let input = "foo\nbar\nbaz";
        let output = run_piped(cmd, input, None).unwrap();
        assert_eq!(output.trim(), "foo");
    }

    #[test]
    fn test_timeout() {
        let cmd = if cfg!(windows) {
            "ping -n 3 127.0.0.1"
        } else {
            "sleep 2"
        };
        let start = std::time::Instant::now();
        let res = run_piped(cmd, "", Some(Duration::from_millis(500)));
        assert!(matches!(res, Err(ShellError::Timeout(_, _))));
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn test_command_failed_includes_command_name() {
        let cmd = if cfg!(windows) {
            "exit 1"
        } else {
            "exit 1"
        };
        let res = run_piped(cmd, "", None);
        match res {
            Err(ShellError::CommandFailed(cmd_str, _)) => {
                assert_eq!(cmd_str, cmd);
            }
            _ => panic!("Expected CommandFailed error"),
        }
    }
}
