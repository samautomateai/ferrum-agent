//! Read-only Git inspection. Never invokes a shell or modifies the index.
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{path::Path, process::Command};

const MAX_OUTPUT: usize = 64 * 1024;

fn run_git(root: &Path, arguments: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("--no-pager")
        .args(arguments)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .context("could not launch git")?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git failed: {}",
            message.chars().take(512).collect::<String>()
        );
    }
    if output.stdout.len() > MAX_OUTPUT {
        bail!("git output exceeds 64 KiB; narrow the request");
    }
    String::from_utf8(output.stdout).context("git returned non-UTF-8 output")
}

fn verify_repository(root: &Path) -> Result<()> {
    let top = run_git(root, &["rev-parse", "--show-toplevel"])?;
    let canonical = std::fs::canonicalize(top.trim()).context("invalid git root")?;
    if canonical != std::fs::canonicalize(root)? {
        bail!("workspace must be the Git repository root");
    }
    Ok(())
}

fn path_argument(args: &Value) -> Result<Option<&str>> {
    let Some(value) = args.get("path") else {
        return Ok(None);
    };
    let path = value.as_str().context("path must be a string")?;
    if path.is_empty() || path.starts_with('-') || path.contains('\0') {
        bail!("invalid Git path");
    }
    let parsed = Path::new(path);
    if parsed.is_absolute()
        || parsed.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::Prefix(_)
            )
        })
    {
        bail!("Git path must be workspace-relative without parent traversal");
    }
    Ok(Some(path))
}

pub(crate) fn definitions() -> Vec<Value> {
    vec![
        json!({"type":"function","function":{"name":"git_status",
            "description":"Read-only Git porcelain status, including untracked files",
            "parameters":{"type":"object","properties":{}}}}),
        json!({"type":"function","function":{"name":"git_diff",
            "description":"Read-only unstaged or staged Git diff (max 64 KiB)",
            "parameters":{"type":"object","properties":{
                "staged":{"type":"boolean"},"path":{"type":"string"}}}}}),
    ]
}

pub(crate) fn execute(root: &Path, name: &str, args: &Value) -> Result<String> {
    verify_repository(root)?;
    match name {
        "git_status" => run_git(
            root,
            &["status", "--porcelain=v1", "--untracked-files=normal"],
        ),
        "git_diff" => {
            let staged = match args.get("staged") {
                None => false,
                Some(value) => value.as_bool().context("staged must be a boolean")?,
            };
            let path = path_argument(args)?;
            let mut argv = vec!["diff", "--no-ext-diff", "--no-textconv", "--no-color"];
            if staged {
                argv.push("--cached");
            }
            if let Some(path) = path {
                argv.extend(["--", path]);
            }
            run_git(root, &argv)
        }
        _ => bail!("unknown Git tool: {name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(root: &Path, args: &[&str]) {
        assert!(Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .status()
            .unwrap()
            .success());
    }

    #[test]
    fn status_and_diff_are_read_only() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "test@example.com"]);
        git(root, &["config", "user.name", "Test"]);
        std::fs::write(root.join("a.txt"), "first\n").unwrap();
        git(root, &["add", "a.txt"]);
        git(root, &["commit", "-qm", "initial"]);
        std::fs::write(root.join("a.txt"), "second\n").unwrap();
        assert!(execute(root, "git_status", &json!({}))
            .unwrap()
            .contains("a.txt"));
        assert!(execute(root, "git_diff", &json!({"path":"a.txt"}))
            .unwrap()
            .contains("+second"));
        assert_eq!(
            execute(root, "git_diff", &json!({"staged":true})).unwrap(),
            ""
        );
    }

    #[test]
    fn rejects_path_injection_and_outside_repository() {
        let temp = tempfile::tempdir().unwrap();
        assert!(execute(temp.path(), "git_status", &json!({})).is_err());
        assert!(path_argument(&json!({"path":"../secret"})).is_err());
        assert!(path_argument(&json!({"path":"--output=/tmp/x"})).is_err());
        assert!(path_argument(&json!({"path":"/tmp/x"})).is_err());
        assert!(path_argument(&json!({"path":2})).is_err());
    }
}
