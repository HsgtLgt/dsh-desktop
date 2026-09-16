use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::install::NPM_ALLOW_SCRIPTS;
use crate::settings::Settings;

/// How long `dsh --dump-config` may run before we treat it as wedged.
const DUMP_CONFIG_TIMEOUT: Duration = Duration::from_secs(20);

/// Ensure the `web` profile can actually boot. dsh manages
/// `profiles/<name>/node_modules` via symlinks and hard-crashes on an
/// inconsistent tree (e.g. left behind by a different dsh version), so we
/// probe with `--dump-config` before every spawn and quarantine a broken
/// profile; the next dsh start rebuilds it fresh.
///
/// `--dump-config` composes the profile but does not load UI plugins, so a
/// stale profile can still pass it and crash later in `dsh web`; the boot path
/// additionally calls [`quarantine_web_profile`] when the child dies with a
/// matching crash signature.
pub fn ensure_web_profile(settings: &Settings, dsh_home: &Path) -> Result<Option<String>, String> {
    let web = dsh_home.join("profiles").join("web");
    if !web.is_dir() {
        return Ok(None);
    }

    if dump_config_ok(settings, dsh_home) {
        return Ok(None);
    }

    Some(quarantine_web_profile(dsh_home)).transpose()
}

/// Rename the web profile and the shared `profiles/node_modules` store aside
/// (`.bak.<stamp>`) so dsh rebuilds them fresh. Rename instead of delete: the
/// profile may hold user patches (cordis.yml…).
pub fn quarantine_web_profile(dsh_home: &Path) -> Result<String, String> {
    let profiles = dsh_home.join("profiles");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut moved: Vec<String> = Vec::new();
    for name in ["web", "node_modules"] {
        let dir = profiles.join(name);
        if dir.exists() {
            let backup = profiles.join(format!("{name}.bak.{stamp}"));
            std::fs::rename(&dir, &backup)
                .map_err(|e| format!("无法备份 {name}：{e}"))?;
            moved.push(format!("{name} → {}", backup.display()));
        }
    }
    if moved.is_empty() {
        return Ok("web profile 不存在，将由 dsh 自动重建".into());
    }

    Ok(format!(
        "检测到 web 配置无法启动，已备份（{}），将自动重建",
        moved.join("；")
    ))
}

/// Crash signatures that mean "the web profile tree itself is broken" —
/// typically a profile left by a different dsh version.
pub fn looks_like_profile_corruption(detail: &str) -> bool {
    const MARKERS: [&str; 5] = [
        "ERR_MODULE_NOT_FOUND",
        "Cannot find package",
        "healProfilesModuleFallback",
        "ensureSymlink",
        ".dsh-module-fallback",
    ];
    MARKERS.iter().any(|m| detail.contains(m))
}

fn dump_config_ok(settings: &Settings, dsh_home: &Path) -> bool {
    matches!(
        run_dsh_with_timeout(settings, dsh_home, &["--profile", "web", "--dump-config"]),
        Some(output) if output.status.success()
    )
}

fn run_dsh_with_timeout(
    settings: &Settings,
    dsh_home: &Path,
    args: &[&str],
) -> Option<std::process::Output> {
    let dsh_path = settings.dsh_path_buf()?;
    let node_exe = settings.node_exe_path()?;
    let node_dir = node_exe.parent()?;
    let prefix = dsh_path.parent()?;

    let mut cmd = Command::new("cmd");
    cmd.arg("/C").arg(&dsh_path);
    for a in args {
        cmd.arg(a);
    }
    let path = std::env::var("PATH").unwrap_or_default();
    cmd.env(
        "PATH",
        format!("{};{};{}", node_dir.display(), prefix.display(), path),
    );
    cmd.env("DSH_HOME", dsh_home);
    cmd.env("npm_config_allow_scripts", NPM_ALLOW_SCRIPTS);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    if let Some(home) = std::env::var_os("USERPROFILE") {
        cmd.current_dir(home);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }

    let mut child = cmd.spawn().ok()?;
    // Drain pipes while running so a chatty dsh can never block on a full buffer.
    if let Some(out) = child.stdout.take() {
        drain_to_void(out);
    }
    if let Some(err) = child.stderr.take() {
        drain_to_void(err);
    }

    let deadline = Instant::now() + DUMP_CONFIG_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(150)),
            Err(_) => return None,
        }
    };

    Some(std::process::Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    })
}

fn drain_to_void<R: Read + Send + 'static>(mut reader: R) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    });
}

pub fn user_dsh_home() -> Option<PathBuf> {
    let profile = std::env::var_os("USERPROFILE")?;
    Some(PathBuf::from(profile).join(".dsh"))
}
