//! Handing a URL to the rest of the system: the browser, or the clipboard.

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

/// Opens `url` in the user's default browser.
///
/// Returns as soon as the helper is launched — `open`, `xdg-open` and `start`
/// all hand off and exit, but none of that should ever block the event loop.
pub fn browser(url: &str) -> Result<()> {
    let (program, args) = opener();
    let mut command = Command::new(program);
    command.args(args).arg(url);
    spawn_detached(command, program)
}

/// Copies `url` to the system clipboard, if this platform has one we can reach.
pub fn clipboard(url: &str) -> Result<()> {
    let candidates = clipboard_commands();
    let mut last: Option<anyhow::Error> = None;

    for (program, args) in candidates {
        match copy_with(program, args, url) {
            Ok(()) => return Ok(()),
            // Wayland, X11 and bare ttys all differ; try the next one.
            Err(err) => last = Some(err),
        }
    }

    match last {
        Some(err) => Err(err),
        None => bail!("no clipboard command is available on this platform"),
    }
}

fn copy_with(program: &str, args: &[&str], url: &str) -> Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("launching {program}"))?;

    child
        .stdin
        .take()
        .context("clipboard command refused stdin")?
        .write_all(url.as_bytes())
        .with_context(|| format!("writing to {program}"))?;

    let status = child
        .wait()
        .with_context(|| format!("waiting for {program}"))?;
    if !status.success() {
        bail!("{program} exited with {status}");
    }
    Ok(())
}

/// Launches a command without blocking, and without leaving a zombie behind.
///
/// A spawned child that is never waited on stays in the process table as a
/// zombie for the life of the reader. Waiting on the event loop would freeze the
/// UI, so a detached thread does it: the helper exits almost immediately, and
/// the thread goes with it.
fn spawn_detached(mut command: Command, program: &str) -> Result<()> {
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("launching {program}"))?;

    std::thread::spawn(move || {
        let mut child = child;
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(target_os = "macos")]
fn opener() -> (&'static str, &'static [&'static str]) {
    ("open", &[])
}

#[cfg(target_os = "windows")]
fn opener() -> (&'static str, &'static [&'static str]) {
    // `start` is a cmd builtin, and its first quoted argument is the window
    // title — hence the empty one before the URL.
    ("cmd", &["/C", "start", ""])
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn opener() -> (&'static str, &'static [&'static str]) {
    ("xdg-open", &[])
}

#[cfg(target_os = "macos")]
fn clipboard_commands() -> Vec<(&'static str, &'static [&'static str])> {
    vec![("pbcopy", &[])]
}

#[cfg(target_os = "windows")]
fn clipboard_commands() -> Vec<(&'static str, &'static [&'static str])> {
    vec![("clip", &[])]
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn clipboard_commands() -> Vec<(&'static str, &'static [&'static str])> {
    // Wayland first, then the two X11 conventions.
    vec![
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_opener_is_the_platform_convention() {
        let (program, _) = opener();
        let expected = if cfg!(target_os = "macos") {
            "open"
        } else if cfg!(target_os = "windows") {
            "cmd"
        } else {
            "xdg-open"
        };
        assert_eq!(program, expected);
    }

    #[test]
    fn every_platform_offers_at_least_one_clipboard_command() {
        assert!(!clipboard_commands().is_empty());
    }

    #[test]
    fn a_missing_helper_is_an_error_rather_than_a_panic() {
        let command = Command::new("rsst-no-such-helper-should-exist");
        assert!(spawn_detached(command, "rsst-no-such-helper-should-exist").is_err());
    }

    #[test]
    fn copying_reports_a_missing_command() {
        assert!(copy_with("rsst-no-such-clipboard", &[], "https://example.com").is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn copying_to_the_real_clipboard_succeeds() {
        // pbcopy is always present on macOS, so this exercises the whole path.
        // Someone running the test suite did not agree to lose their clipboard,
        // so put back whatever was in it.
        let before = Command::new("pbpaste")
            .output()
            .ok()
            .map(|out| out.stdout)
            .unwrap_or_default();

        let result = clipboard("https://example.com/rsst-test");

        if let Ok(mut child) = Command::new("pbcopy").stdin(Stdio::piped()).spawn()
            && let Some(mut stdin) = child.stdin.take()
        {
            let _ = stdin.write_all(&before);
            drop(stdin);
            let _ = child.wait();
        }

        assert!(result.is_ok());
    }
}
