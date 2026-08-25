use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_PORT: u16 = 3080;
pub const DEFAULT_PROBE_TIMEOUT_MS: u64 = 90_000;
pub const DEFAULT_PROBE_INTERVAL_MS: u64 = 500;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub schema_version: u32,
    pub port: u16,
    /// Absolute path to node.exe
    pub node_exe: Option<String>,
    /// Absolute path to dsh.cmd / dsh entry
    pub dsh_path: Option<String>,
    /// Recorded installed dsh version (informational)
    pub dsh_version: Option<String>,
    pub autostart: bool,
    /// Autostart should stay tray-only
    pub start_minimized: bool,
    pub probe_timeout_ms: u64,
    pub probe_interval_ms: u64,
    /// Production hot path must ignore this; kept for emergency/dev only
    #[serde(default)]
    pub npx_allowed: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            port: DEFAULT_PORT,
            node_exe: None,
            dsh_path: None,
            dsh_version: None,
            autostart: false,
            start_minimized: true,
            probe_timeout_ms: DEFAULT_PROBE_TIMEOUT_MS,
            probe_interval_ms: DEFAULT_PROBE_INTERVAL_MS,
            npx_allowed: false,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        let Ok(content) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        match serde_json::from_str::<Settings>(&content) {
            Ok(mut s) => {
                if s.schema_version == 0 {
                    s.schema_version = SCHEMA_VERSION;
                }
                s
            }
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn node_exe_path(&self) -> Option<PathBuf> {
        self.node_exe.as_ref().map(PathBuf::from)
    }

    pub fn dsh_path_buf(&self) -> Option<PathBuf> {
        self.dsh_path.as_ref().map(PathBuf::from)
    }

    pub fn runtime_ready(&self) -> bool {
        match (self.node_exe_path(), self.dsh_path_buf()) {
            (Some(n), Some(d)) => n.is_file() && d.is_file(),
            _ => false,
        }
    }
}
