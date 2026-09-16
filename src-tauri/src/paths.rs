use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

/// %LOCALAPPDATA%/dsh-desktop/ layout helpers
#[derive(Clone)]
pub struct AppPaths {
    pub root: PathBuf,
}

impl AppPaths {
    pub fn from_app(app: &AppHandle) -> Result<Self, String> {
        let root = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?;
        Ok(Self { root })
    }

    pub fn ensure(&self) -> Result<(), String> {
        for p in [
            self.root.clone(),
            self.runtime_dir(),
            self.node_root(),
            self.npm_prefix(),
            self.logs_dir(),
            self.state_dir(),
        ] {
            std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn runtime_dir(&self) -> PathBuf {
        self.root.join("runtime")
    }

    pub fn node_root(&self) -> PathBuf {
        self.runtime_dir().join("node")
    }

    pub fn npm_prefix(&self) -> PathBuf {
        self.runtime_dir().join("npm-prefix")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn state_dir(&self) -> PathBuf {
        self.root.join("state")
    }

    pub fn last_error_file(&self) -> PathBuf {
        self.state_dir().join("last-error.json")
    }

    pub fn boot_log_file(&self) -> PathBuf {
        let day = chrono_like_date();
        self.logs_dir().join(format!("boot-{day}.log"))
    }

    /// Old isolated home (0.1.x shells); merged into ~/.dsh once, then ignored.
    pub fn legacy_dsh_home(&self) -> PathBuf {
        self.root.join("dsh-home")
    }
}

fn chrono_like_date() -> String {
    // Avoid extra chrono dep: local YYYYMMDD via Windows-ish formatting
    use std::time::SystemTime;
    let Ok(dur) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) else {
        return "unknown".into();
    };
    // Rough UTC date is fine for log names; enough for diagnostics
    let days = dur.as_secs() / 86400;
    // 1970-01-01 + days — keep simple: use a fixed format from local time via cmd? 
    // Prefer: format from Local via `time` crate — we don't have it.
    // Use PowerShell-free: store as unix-day stamped file is OK, but README said YYYYMMDD.
    // Parse via `cmd /c date /t` is fragile. Use simple UTC date calculation:
    let (y, m, d) = civil_from_days(days as i64);
    format!("{y:04}{m:02}{d:02}")
}

/// Howard Hinnant civil_from_days (UTC)
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

/// Find the directory that contains node.exe under a portable extract root.
pub fn find_node_dir(root: &Path) -> Option<PathBuf> {
    if root.join("node.exe").is_file() {
        return Some(root.to_path_buf());
    }
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() && p.join("node.exe").is_file() {
            return Some(p);
        }
    }
    None
}

/// Expected dsh.cmd location after `npm install -g --prefix <prefix>`.
pub fn expected_dsh_cmd(npm_prefix: &Path) -> PathBuf {
    npm_prefix.join("dsh.cmd")
}
