use std::path::Path;

/// One-time merge of the old isolated `dsh-home/` (0.1.x shell layout) into the
/// user's real `~/.dsh`, so desktop and CLI share sessions/settings/credentials.
/// Missing-file-only: never overwrites newer CLI data.
///
/// `profiles/` is deliberately NOT merged: dsh heals and rebuilds profiles per
/// version, and copying an old profile tree can crash boot (symlink fallback
/// invariant). Profile repair is handled by `profile_repair`.
pub fn merge_legacy_desktop_home(
    legacy_home: &Path,
    user_home: &Path,
) -> Result<Option<String>, String> {
    if !legacy_home.is_dir() || same_path(legacy_home, user_home) {
        return Ok(None);
    }
    let meaningful = legacy_home.join("sessions").is_dir()
        || legacy_home.join("storages").is_dir()
        || legacy_home.join("settings.yaml").is_file();
    if !meaningful {
        return Ok(None);
    }

    std::fs::create_dir_all(user_home).map_err(|e| e.to_string())?;
    let mut actions: Vec<String> = Vec::new();

    for name in [
        "settings.yaml",
        ".credentials.yaml",
        ".env",
        ".anonymous-user-id",
    ] {
        if copy_file_if_missing(&legacy_home.join(name), &user_home.join(name))? {
            actions.push(format!("added {name}"));
        }
    }
    for dir in ["sessions", "storages", ".agent-presets"] {
        if merge_dir_missing_files(&legacy_home.join(dir), &user_home.join(dir))? {
            actions.push(format!("merged {dir}"));
        }
    }

    if actions.is_empty() {
        return Ok(None);
    }
    Ok(Some(format!(
        "从 {} 合并到 {}：{}",
        legacy_home.display(),
        user_home.display(),
        actions.join("；")
    )))
}

fn same_path(a: &Path, b: &Path) -> bool {
    let a = std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let b = std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    a == b
}

fn copy_file_if_missing(src: &Path, dest: &Path) -> Result<bool, String> {
    if !src.is_file() || dest.exists() {
        return Ok(false);
    }
    copy_file(src, dest)?;
    Ok(true)
}

fn copy_file(src: &Path, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::copy(src, dest)
        .map(|_| ())
        .map_err(|e| format!("copy {} → {}: {e}", src.display(), dest.display()))
}

fn merge_dir_missing_files(src: &Path, dest: &Path) -> Result<bool, String> {
    if !src.is_dir() {
        return Ok(false);
    }
    let mut any = false;
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        if ty.is_dir() {
            if merge_dir_missing_files(&from, &to)? {
                any = true;
            }
        } else if ty.is_file() && !to.exists() {
            copy_file(&from, &to)?;
            any = true;
        }
    }
    Ok(any)
}
