//! Central pre-dispatch policy for built-in and extension tools.
//! This is an authorization gate, not an OS sandbox.
use anyhow::{bail, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolOrigin {
    Native,
    Extension,
}

/// Fail closed for unknown native tools and for extensions without explicit opt-in.
/// Extension code is trusted-only even after opt-in: worker processes are not sandboxed.
pub(crate) fn authorize(
    name: &str,
    origin: ToolOrigin,
    allow_write: bool,
    allow_extension_tools: bool,
) -> Result<()> {
    match origin {
        ToolOrigin::Extension => {
            if !allow_extension_tools {
                bail!("extension execution denied; pass --allow-extension-tools for trusted extensions");
            }
            if name.trim().is_empty() {
                bail!("empty extension tool name");
            }
        }
        ToolOrigin::Native => match name {
            "read_file" | "list_files" | "grep" | "git_status" | "git_diff" => {}
            "write_file" | "edit_file" if allow_write => {}
            "write_file" | "edit_file" => {
                bail!("write disabled; pass --allow-write");
            }
            _ => bail!("unknown native tool: {name}"),
        },
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_native_tools_are_allowed() {
        for name in ["read_file", "list_files", "grep", "git_status", "git_diff"] {
            authorize(name, ToolOrigin::Native, false, false).unwrap();
        }
    }

    #[test]
    fn native_writes_require_opt_in() {
        for name in ["write_file", "edit_file"] {
            assert!(authorize(name, ToolOrigin::Native, false, true).is_err());
            authorize(name, ToolOrigin::Native, true, false).unwrap();
        }
    }

    #[test]
    fn extension_tools_require_separate_opt_in() {
        assert!(authorize("custom_tool", ToolOrigin::Extension, true, false).is_err());
        authorize("custom_tool", ToolOrigin::Extension, false, true).unwrap();
        assert!(authorize("", ToolOrigin::Extension, false, true).is_err());
    }

    #[test]
    fn unknown_native_tools_fail_closed() {
        assert!(authorize("custom_tool", ToolOrigin::Native, true, true).is_err());
    }
}
