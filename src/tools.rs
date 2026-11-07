use super::*;
use crate::{edit, search};
pub(super) fn confined(root: &Path, input: &str, writing: bool) -> Result<PathBuf> {
    let root = root.canonicalize()?;
    let relative = Path::new(input);
    if relative.is_absolute() || relative.components().any(|part| matches!(part,
        std::path::Component::ParentDir | std::path::Component::Prefix(_))) {
        bail!("absolute paths and parent traversal are forbidden");
    }
    if relative.as_os_str().is_empty() { bail!("empty path"); }
    let path = root.join(relative);
    if writing && !path.exists() {
        let parent = path.parent().context("missing parent")?.canonicalize()?;
        if !parent.starts_with(&root) { bail!("path escapes workspace"); }
        return Ok(parent.join(path.file_name().context("missing filename")?));
    }
    let canonical = path.canonicalize().context("path not found")?;
    if !canonical.starts_with(&root) { bail!("path escapes workspace"); }
    Ok(canonical)
}


pub(super) fn atomic_replace(path: &Path, data: &[u8]) -> Result<()> {
    use std::fs::OpenOptions;
    let parent = path.parent().context("missing parent")?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    if let Ok(metadata) = fs::metadata(path) { tmp.as_file().set_permissions(metadata.permissions())?; }
    tmp.write_all(data)?;
    tmp.as_file().sync_all()?;
    // On Windows, replacing an existing file may require a platform-specific operation.
    tmp.persist(path).map_err(|e| anyhow!("atomic replace failed: {}", e.error))?;
    Ok(())
}

pub(super) fn tool_definitions() -> Value {
    json!([
        {"type":"function","function":{"name":"read_file",
        "description":"Read UTF-8 file in workspace (max 64 KiB)","parameters":{"type":"object",
        "properties":{"path":{"type":"string"}},"required":["path"]}}},
        {"type":"function","function":{"name":"list_files",
        "description":"List workspace files honoring gitignore, max 200","parameters":{"type":"object",
        "properties":{"pattern":{"type":"string","description":"Optional substring filter"}}}}},
        {"type":"function","function":{"name":"grep",
        "description":"Search UTF-8 text with Rust regex, max 100 matches","parameters":{"type":"object",
        "properties":{"pattern":{"type":"string"}},"required":["pattern"]}}},
        {"type":"function","function":{"name":"write_file",
        "description":"Write UTF-8 file (requires --allow-write)","parameters":{"type":"object",
        "properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}}},
        {"type":"function","function":{"name":"edit_file",
        "description":"Replace exactly one occurrence (requires --allow-write)","parameters":{"type":"object",
        "properties":{"path":{"type":"string"},"old":{"type":"string"},"new":{"type":"string"},
        "expected_sha256":{"type":"string","description":"Optional SHA-256 of the exact original file"}},
        "required":["path","old","new"]}}}
    ])
}

pub(super) fn execute(root: &Path, write_enabled: bool, name: &str, args: &Value) -> Result<String> {
    let get = |key: &str| -> Result<&str> {
        args.get(key).and_then(Value::as_str).ok_or_else(|| anyhow!("missing {key}"))
    };
    match name {
        "read_file" => {
            let bytes = fs::read(confined(root, get("path")?, false)?)?;
            if bytes.len() > 65536 { bail!("file exceeds 64 KiB limit"); }
            Ok(String::from_utf8(bytes).context("not UTF-8")?)
        }
        "list_files" => {
            let filter = args.get("pattern").and_then(Value::as_str).unwrap_or("");
            let mut out = Vec::new();
            for entry in ignore::Walk::new(root).filter_map(Result::ok) {
                if !entry.file_type().is_some_and(|t| t.is_file()) { continue; }
                if !entry.path().canonicalize().is_ok_and(|p| {
                    p.starts_with(root.canonicalize().unwrap_or_else(|_| root.to_path_buf()))
                }) { continue; }
                let rel = entry.path().strip_prefix(root)?.to_string_lossy().to_string();
                if rel.contains(filter) { out.push(rel); }
                if out.len() >= 200 { break; }
            }
            Ok(out.join("\n"))
        }
        "grep" => search::grep(root, get("pattern")?),
        "write_file" => {
            if !write_enabled { bail!("write disabled; pass --allow-write"); }
            let path = confined(root, get("path")?, true)?;
            let content = get("content")?;
            if content.len() > 1024 * 1024 { bail!("write exceeds 1 MiB limit"); }
            atomic_replace(&path, content.as_bytes())?;
            Ok("written".into())
        }
        "edit_file" => {
            if !write_enabled { bail!("edit disabled; pass --allow-write"); }
            let path = confined(root, get("path")?, false)?;
            let original = fs::read(&path)?;
            let expected = args.get("expected_sha256").map(|value| {
                value.as_str().context("expected_sha256 must be a string")
            }).transpose()?;
            let updated = edit::replace_once(&original, get("old")?, get("new")?, expected)?;
            // Best-effort conflict check, not atomic compare-and-swap.
            if fs::read(&path)? != original { bail!("edit conflict: file changed during edit"); }
            atomic_replace(&path, &updated)?;
            Ok("edited".into())
        }
        _ => bail!("unknown tool: {name}"),
    }
}

