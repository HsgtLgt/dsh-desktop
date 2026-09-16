use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::paths::{expected_dsh_cmd, find_node_dir, AppPaths};
use crate::settings::Settings;

/// npm 11+/12 blocks dependency install scripts by default.
pub const NPM_ALLOW_SCRIPTS: &str =
    "@deepseek-ai/dsh-subprocess-local,koffi,node-pty,@google/genai,protobufjs";

/// The dsh release this shell build is validated against. Bump when adopting
/// a new official release. Installed as an exact spec so upgrades are
/// reproducible; the boot path auto-upgrades once when the installed version
/// is older than this.
pub const DSH_NPM_SPEC: &str = "@deepseek-ai/dsh@0.1.6-alpha.1";

/// Version portion of [`DSH_NPM_SPEC`], e.g. "0.1.6-alpha.1".
pub fn desired_dsh_version() -> &'static str {
    DSH_NPM_SPEC.rsplit('@').next().unwrap_or(DSH_NPM_SPEC)
}

/// True when the installed dsh is older than [`desired_dsh_version`] and an
/// upgrade would move it forward. Never downgrades a newer install.
pub fn should_upgrade_dsh(npm_prefix: &Path) -> bool {
    match read_dsh_version(npm_prefix) {
        Some(installed) => version_gt(desired_dsh_version(), &installed),
        None => true,
    }
}

/// Minimal semver-ish ordering: numeric major.minor.patch, then prerelease
/// (release > rc > beta > alpha, later numbers win). Enough to decide
/// "installed is behind the pin"; unknown shapes compare lexicographically.
fn version_gt(a: &str, b: &str) -> bool {
    let (ac, ap) = split_prerelease(a);
    let (bc, bp) = split_prerelease(b);
    for (x, y) in ac.iter().zip(bc.iter()) {
        if x != y {
            return x > y;
        }
    }
    if ac != bc {
        return ac > bc;
    }
    match (ap, bp) {
        (None, None) => false,
        (Some(_), None) => false, // prerelease < release
        (None, Some(_)) => true,
        (Some(x), Some(y)) => pre_gt(x, y),
    }
}

fn split_prerelease(v: &str) -> ([u64; 3], Option<&str>) {
    let (core, pre) = match v.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (v, None),
    };
    let mut nums = [0u64; 3];
    for (i, part) in core.split('.').take(3).enumerate() {
        nums[i] = part.parse().unwrap_or(0);
    }
    (nums, pre)
}

fn pre_gt(a: &str, b: &str) -> bool {
    fn rank(tok: &str) -> u64 {
        match tok {
            "alpha" => 0,
            "beta" => 1,
            "rc" => 2,
            _ => 3,
        }
    }
    let av: Vec<&str> = a.split('.').collect();
    let bv: Vec<&str> = b.split('.').collect();
    for i in 0..av.len().max(bv.len()) {
        let x = av.get(i);
        let y = bv.get(i);
        match (x, y) {
            (None, None) => return false,
            (None, Some(_)) => return false, // shorter prerelease is lower
            (Some(_), None) => return true,
            (Some(xt), Some(yt)) => {
                let xn = xt.parse::<u64>().ok();
                let yn = yt.parse::<u64>().ok();
                match (xn, yn) {
                    (Some(xn), Some(yn)) if xn != yn => return xn > yn,
                    (Some(_), Some(_)) => {}
                    _ if xt != yt => return rank(xt) > rank(yt),
                    _ => {}
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desired_version_is_parsed_from_spec() {
        assert_eq!(desired_dsh_version(), "0.1.6-alpha.1");
    }

    #[test]
    fn upgrade_targets_newer_versions_only() {
        let d = |p: &str| should_upgrade_to(p);
        assert!(d("0.1.1-rc.2")); // the shipped-0.1.0-era runtime
        assert!(d("0.1.5-rc.2"));
        assert!(d("0.1.6-alpha.0"));
        assert!(!d("0.1.6-alpha.1")); // exact pin
        assert!(!d("0.1.6-alpha.2")); // newer alpha — never downgrade
        assert!(!d("0.1.6-rc.1")); // rc > alpha
        assert!(!d("0.1.6")); // release > alpha
        assert!(!d("0.1.7-alpha.1")); // future release — never downgrade
        assert!(d("garbage")); // unparsable installed → try to fix it
    }

    fn should_upgrade_to(installed: &str) -> bool {
        version_gt(desired_dsh_version(), installed)
    }
}

const NODE_SETUP_PS1: &str = r#"$ErrorActionPreference = 'Stop'
$idx = curl.exe -sL --fail 'https://nodejs.org/dist/index.json'
$json = $idx | ConvertFrom-Json
$lts = $json | Where-Object { $_.lts -ne $false } | Select-Object -First 1
$ver = $lts.version
$url = 'https://nodejs.org/dist/' + $ver + '/node-' + $ver + '-win-x64.zip'
$zip = Join-Path '__NODE_ROOT__' 'node.zip'
curl.exe -sL --fail -o $zip $url
tar.exe -xf $zip -C '__NODE_ROOT__'
if ($LASTEXITCODE -ne 0) { throw '解压 Node.js 失败' }
Remove-Item $zip -Force
"#;

pub struct InstallResult {
    pub node_exe: PathBuf,
    pub dsh_path: PathBuf,
    pub dsh_version: Option<String>,
}

/// Install portable Node under runtime/node and pin @deepseek-ai/dsh into npm-prefix.
pub fn install_runtime(paths: &AppPaths, progress: &mut dyn FnMut(&str)) -> Result<InstallResult, String> {
    paths.ensure()?;
    progress("正在下载并安装便携版 Node.js…");
    let node_dir = ensure_portable_node(&paths.node_root())?;
    let node_exe = node_dir.join("node.exe");
    if !node_exe.is_file() {
        return Err("未找到 node.exe".into());
    }

    progress("正在安装 DeepSeek Harness（钉死到应用目录）…");
    let dsh_path = install_dsh(&node_dir, &paths.npm_prefix())?;
    let dsh_version = read_dsh_version(&paths.npm_prefix());

    Ok(InstallResult {
        node_exe,
        dsh_path,
        dsh_version,
    })
}

/// Re-install / upgrade dsh into the private npm prefix using pinned node.
pub fn upgrade_dsh(settings: &Settings, paths: &AppPaths) -> Result<InstallResult, String> {
    let node_exe = settings
        .node_exe_path()
        .filter(|p| p.is_file())
        .ok_or_else(|| "settings 中缺少有效的 nodeExe，请先完成安装向导".to_string())?;
    let node_dir = node_exe
        .parent()
        .ok_or_else(|| "nodeExe 路径异常".to_string())?
        .to_path_buf();
    paths.ensure()?;
    let dsh_path = install_dsh(&node_dir, &paths.npm_prefix())?;
    let dsh_version = read_dsh_version(&paths.npm_prefix());
    Ok(InstallResult {
        node_exe,
        dsh_path,
        dsh_version,
    })
}

fn ensure_portable_node(node_root: &Path) -> Result<PathBuf, String> {
    if let Some(existing) = find_node_dir(node_root) {
        return Ok(existing);
    }
    std::fs::create_dir_all(node_root).map_err(|e| e.to_string())?;
    let ps = NODE_SETUP_PS1.replace("__NODE_ROOT__", &node_root.display().to_string());
    let ps_path = node_root.join("setup-node.ps1");
    std::fs::write(&ps_path, ps).map_err(|e| e.to_string())?;

    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            ps_path.to_str().unwrap_or(""),
        ])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("无法启动 PowerShell：{e}"))?;

    let _ = std::fs::remove_file(&ps_path);
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: String = err.chars().rev().take(800).collect::<Vec<_>>().into_iter().rev().collect();
        return Err(if tail.is_empty() {
            format!("Node.js 安装失败，退出码：{}", out.status)
        } else {
            format!("Node.js 安装失败：{tail}")
        });
    }

    find_node_dir(node_root).ok_or_else(|| "Node.js 安装后未找到 node.exe".to_string())
}

fn install_dsh(node_dir: &Path, npm_prefix: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(npm_prefix).map_err(|e| e.to_string())?;
    let npm_cmd = node_dir.join("npm.cmd");
    if !npm_cmd.is_file() {
        return Err(format!("未找到 npm：{}", npm_cmd.display()));
    }

    let mut cmd = Command::new("cmd");
    cmd.arg("/C")
        .arg(&npm_cmd)
        .arg("install")
        .arg("-g")
        .arg("--prefix")
        .arg(npm_prefix)
        .arg(format!("--allow-scripts={NPM_ALLOW_SCRIPTS}"))
        .arg(DSH_NPM_SPEC)
        .env("npm_config_allow_scripts", NPM_ALLOW_SCRIPTS)
        .stdin(Stdio::null());

    // Ensure npm finds this node first
    let path = std::env::var("PATH").unwrap_or_default();
    cmd.env(
        "PATH",
        format!("{};{}", node_dir.display(), path),
    );

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }

    let out = cmd
        .output()
        .map_err(|e| format!("无法执行 npm install：{e}"))?;
    if !out.status.success() {
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let tail: String = combined
            .chars()
            .rev()
            .take(1200)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        return Err(format!("安装 dsh 失败：{tail}"));
    }

    let dsh = expected_dsh_cmd(npm_prefix);
    if !dsh.is_file() {
        // Some npm layouts put binaries under prefix/bin
        let alt = npm_prefix.join("bin").join("dsh.cmd");
        if alt.is_file() {
            return Ok(alt);
        }
        return Err(format!(
            "安装完成但未找到 dsh.cmd（期望 {}）",
            dsh.display()
        ));
    }
    Ok(dsh)
}

fn read_dsh_version(npm_prefix: &Path) -> Option<String> {
    let pkg = npm_prefix
        .join("node_modules")
        .join("@deepseek-ai")
        .join("dsh")
        .join("package.json");
    let content = std::fs::read_to_string(pkg).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    v.get("version")?.as_str().map(|s| s.to_string())
}

/// Try to recover absolute paths from an existing on-disk runtime (after upgrade/wipe of settings).
pub fn recover_from_disk(paths: &AppPaths) -> Option<(PathBuf, PathBuf, Option<String>)> {
    let node_dir = find_node_dir(&paths.node_root())?;
    let node_exe = node_dir.join("node.exe");
    if !node_exe.is_file() {
        return None;
    }
    let dsh = expected_dsh_cmd(&paths.npm_prefix());
    let dsh = if dsh.is_file() {
        dsh
    } else {
        let alt = paths.npm_prefix().join("bin").join("dsh.cmd");
        if alt.is_file() {
            alt
        } else {
            return None;
        }
    };
    let ver = read_dsh_version(&paths.npm_prefix());
    Some((node_exe, dsh, ver))
}
