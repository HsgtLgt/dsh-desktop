use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Url, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_notification::NotificationExt;

const DEFAULT_PORT: u16 = 3080;
const BOOT_TIMEOUT: Duration = Duration::from_secs(240);
const QUICK_ASK_SHORTCUT: &str = "Alt+Space";
const QUICK_ASK_SHORTCUT_FALLBACK: &str = "Alt+Shift+Space";
/// Capped size of the shared boot-process output buffer.
const MAX_BOOT_LOG: usize = 8192;
/// npm 11+/12 refuses to run dependency install scripts by default. dsh's
/// terminal stack (node-pty, koffi) needs its postinstall scripts, so pass
/// the allow-list through to npx's underlying install. Ignored by older npm.
const NPM_ALLOW_SCRIPTS: &str =
    "@deepseek-ai/dsh-subprocess-local,koffi,node-pty,@google/genai,protobufjs";
const NODE_SETUP_PS1: &str = r#"$ErrorActionPreference = 'Stop'
$idx = curl.exe -sL --fail 'https://nodejs.org/dist/index.json'
$json = $idx | ConvertFrom-Json
$lts = $json | Where-Object { $_.lts -ne $false } | Select-Object -First 1
$ver = $lts.version
$url = 'https://nodejs.org/dist/' + $ver + '/node-' + $ver + '-win-x64.zip'
$zip = '__NODE_ROOT__/node.zip'
curl.exe -sL --fail -o $zip $url
tar.exe -xf $zip -C '__NODE_ROOT__'
if ($LASTEXITCODE -ne 0) { throw '解压 Node.js 失败' }
Remove-Item $zip -Force
"#;

struct AppState {
    /// The dsh child process we spawned (None if we connected to an existing one).
    child: Mutex<Option<Child>>,
    /// Shared capped buffer of the boot process's stdout+stderr, so failures
    /// (e.g. npx errors) can be surfaced to the user instead of a bare exit code.
    boot_log: Arc<Mutex<String>>,
    /// Directory of a portable Node.js we installed ourselves.
    node_dir: Mutex<Option<PathBuf>>,
    /// The running quick-ask headless task, if any.
    quick_ask_child: Mutex<Option<Child>>,
    quitting: AtomicBool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BootEvent {
    stage: String,
    message: String,
    detail: Option<String>,
}

fn emit(app: &AppHandle, stage: &str, message: &str, detail: Option<String>) {
    let _ = app.emit(
        "boot-status",
        BootEvent {
            stage: stage.into(),
            message: message.into(),
            detail,
        },
    );
}

fn port() -> u16 {
    std::env::var("DSH_DESKTOP_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

/// Minimal HTTP probe: returns true if a page answering with `<!doctype` is
/// served on 127.0.0.1:port. That's the dsh web UI.
fn probe(port: u16) -> bool {
    let Ok(mut s) = TcpStream::connect(("127.0.0.1", port)) else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_millis(1200)));
    let _ = s.set_write_timeout(Some(Duration::from_millis(1200)));
    if s.write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut buf = [0u8; 512];
    let Ok(n) = s.read(&mut buf) else {
        return false;
    };
    let head = String::from_utf8_lossy(&buf[..n]).to_ascii_lowercase();
    head.contains(" 200 ") && head.contains("<!doctype")
}

fn system_node_version() -> Option<String> {
    let out = Command::new("node").arg("--version").output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        None
    }
}

/// True when a `dsh` executable is resolvable on PATH (a global npm install).
/// Preferring it over npx sidesteps npm 12's broken npx cache-lock
/// (ECOMPROMISED) and its default block on dependency install scripts.
fn find_global_dsh() -> bool {
    let probe = if cfg!(windows) { "where" } else { "which" };
    Command::new(probe)
        .arg("dsh")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Drain a child's stdout/stderr into the shared boot log (capped), so the
/// pipe can never fill up and stall the server, and the tail stays available
/// for error reporting.
fn tee_into_log<R: Read + Send + 'static>(mut reader: R, log: Arc<Mutex<String>>) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let s = String::from_utf8_lossy(&buf[..n]).to_string();
                    if std::env::var_os("DSH_DESKTOP_LOG_DIR").is_some() {
                        debug_log(&format!("[dsh] {s}"));
                    }
                    let mut log = log.lock().unwrap();
                    log.push_str(&s);
                    if log.len() > MAX_BOOT_LOG {
                        let cut = log.len() - MAX_BOOT_LOG;
                        log.drain(..cut);
                    }
                }
            }
        }
    });
}

/// Last ~1500 chars of the boot process output, for error details.
fn boot_log_tail(app: &AppHandle) -> String {
    let state = app.state::<AppState>();
    let log = state.boot_log.lock().unwrap();
    let trimmed = log.trim_end();
    trimmed
        .chars()
        .rev()
        .take(1500)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// Spawn the dsh web server (`dsh web --port <port>`).
///
/// Launcher order:
/// 1. A globally installed `dsh` on PATH — bypasses npx entirely, which
///    avoids npm 12's broken npx cache-lock (ECOMPROMISED) and its default
///    refusal to run dependency install scripts (node-pty/koffi need them).
/// 2. `npx --yes @deepseek-ai/dsh` — either the portable Node's npx
///    (`node_dir`) or the system npx.
fn spawn_dsh(
    node_dir: Option<&Path>,
    port: u16,
    boot_log: Arc<Mutex<String>>,
) -> std::io::Result<Child> {
    let mut cmd = Command::new("cmd");
    cmd.arg("/C");
    let use_global = find_global_dsh();
    if use_global {
        debug_log("spawn_dsh: using globally installed `dsh`");
        cmd.arg("dsh");
    } else if let Some(dir) = node_dir {
        debug_log("spawn_dsh: using portable npx");
        cmd.arg(dir.join("npx.cmd"));
        let path = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", format!("{};{}", dir.display(), path));
        cmd.arg("--yes").arg("@deepseek-ai/dsh");
    } else {
        debug_log("spawn_dsh: using system npx");
        cmd.arg("npx").arg("--yes").arg("@deepseek-ai/dsh");
    }
    // npm 11+/12 blocks dependency install scripts by default; let npx's
    // underlying install still run dsh's required native-module scripts.
    // Ignored silently by older npm versions.
    cmd.env("npm_config_allow_scripts", NPM_ALLOW_SCRIPTS);
    cmd.arg("web")
        .arg("--port")
        .arg(port.to_string())
        .stdin(Stdio::null());
    // Capture stdout/stderr into the boot log (drained by tee threads, so the
    // pipe can never fill up). Previously they were discarded, leaving only a
    // bare "进程已退出" with no diagnostics.
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: don't flash a console window.
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd.spawn()?;
    if let Some(out) = child.stdout.take() {
        tee_into_log(out, boot_log.clone());
    }
    if let Some(err) = child.stderr.take() {
        tee_into_log(err, boot_log);
    }
    Ok(child)
}

fn debug_log(msg: &str) {
    if let Some(dir) = std::env::var_os("DSH_DESKTOP_LOG_DIR") {
        let path = std::path::Path::new(&dir).join("dsh-desktop.log");
        use std::io::Write as _;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{}", msg);
        }
    }
    eprintln!("[dsh-desktop] {msg}");
}

fn navigate_to_dsh(app: &AppHandle) {
    let p = port();
    let url = format!("http://127.0.0.1:{p}");
    if let Some(w) = app.get_webview_window("main") {
        let res = w.navigate(Url::parse(&url).expect("valid URL"));
        debug_log(&format!("navigate to {url}: {:?}", res.map(|_| "ok")));
    }
}

fn kill_spawned_child(app: &AppHandle) {
    let state = app.state::<AppState>();
    let child = {
        let mut guard = state.child.lock().unwrap();
        guard.take()
    };
    if let Some(mut child) = child {
        #[cfg(windows)]
        {
            // taskkill /T kills the process tree (cmd -> npx -> node).
            let _ = Command::new("taskkill")
                .args(["/PID", &child.id().to_string(), "/T", "/F"])
                .status();
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Boot flow, run on a background thread so the UI can show progress.
fn boot(app: AppHandle) {
    std::thread::spawn(move || {
        let p = port();
        debug_log(&format!("boot: detecting dsh on port {p}"));
        emit(&app, "detecting", "正在检测 DSH 服务…", None);

        if probe(p) {
            debug_log("boot: dsh already running, navigating");
            emit(&app, "ready", "DSH 已在运行，正在打开…", None);
            navigate_to_dsh(&app);
            return;
        }
        debug_log("boot: dsh not running, checking node");

        let state = app.state::<AppState>();
        let node_dir = state.node_dir.lock().unwrap().clone();

        match (node_dir, system_node_version()) {
            (Some(dir), _) => {
                emit(&app, "starting", "正在使用内置 Node.js 启动 DSH…", None);
                start_dsh(&app, Some(&dir));
            }
            (None, Some(ver)) => {
                emit(
                    &app,
                    "starting",
                    &format!("检测到 Node.js {ver}，正在启动 DSH…"),
                    None,
                );
                start_dsh(&app, None);
            }
            (None, None) => {
                emit(
                    &app,
                    "need-node",
                    "未检测到 Node.js 运行时",
                    Some("DSH 桌面版需要 Node.js 才能启动服务。你可以一键安装便携版，或手动安装。".into()),
                );
            }
        }
    });
}

fn start_dsh(app: &AppHandle, node_dir: Option<&Path>) {
    let p = port();
    debug_log(&format!(
        "start_dsh: spawning dsh web on port {p} (node_dir={:?})",
        node_dir.map(|d| d.display().to_string())
    ));
    let boot_log = app.state::<AppState>().boot_log.clone();
    let child = match spawn_dsh(node_dir, p, boot_log) {
        Ok(c) => c,
        Err(e) => {
            emit(
                app,
                "error",
                "启动 DSH 失败",
                Some(format!("无法启动进程：{e}")),
            );
            return;
        }
    };
    *app.state::<AppState>().child.lock().unwrap() = Some(child);

    let deadline = Instant::now() + BOOT_TIMEOUT;
    while Instant::now() < deadline {
        if probe(p) {
            debug_log("start_dsh: dsh is ready");
            emit(app, "ready", "DSH 已就绪，正在打开…", None);
            navigate_to_dsh(app);
            return;
        }
        // If our child died, report it (with the captured output tail).
        let died = app
            .state::<AppState>()
            .child
            .lock()
            .unwrap()
            .as_mut()
            .and_then(|c| c.try_wait().ok().flatten());
        if let Some(status) = died {
            let tail = boot_log_tail(app);
            let detail = if tail.is_empty() {
                format!("退出码：{status}")
            } else {
                format!("退出码：{status}\n\n--- dsh 输出尾部 ---\n{tail}")
            };
            emit(app, "error", "DSH 进程已退出", Some(detail));
            return;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let tail = boot_log_tail(app);
    let detail = if tail.is_empty() {
        "在限定时间内未能就绪，请检查网络后重试。".to_string()
    } else {
        format!("在限定时间内未能就绪。\n\n--- dsh 输出尾部 ---\n{tail}")
    };
    emit(app, "error", "DSH 启动超时", Some(detail));
}

/// Install a portable Node.js (Windows x64 zip) under the app data dir,
/// then re-run the boot flow. Runs on a background thread.
fn install_portable_node(app: AppHandle) {
    std::thread::spawn(move || {
        emit(&app, "installing", "正在准备便携版 Node.js…", None);
        let root = match app.path().app_data_dir() {
            Ok(d) => d.join("node"),
            Err(e) => {
                emit(&app, "error", "获取应用数据目录失败", Some(e.to_string()));
                return;
            }
        };
        if let Err(e) = std::fs::create_dir_all(&root) {
            emit(&app, "error", "创建目录失败", Some(e.to_string()));
            return;
        }

        let ps = NODE_SETUP_PS1.replace("__NODE_ROOT__", &root.display().to_string());
        let ps_path = root.join("setup-node.ps1");
        if let Err(e) = std::fs::write(&ps_path, ps) {
            emit(&app, "error", "写入安装脚本失败", Some(e.to_string()));
            return;
        }

        emit(&app, "installing", "正在下载并安装 Node.js（约 30MB）…", None);
        let result = Command::new("powershell")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                ps_path.to_str().unwrap_or(""),
            ])
            .stdin(Stdio::null())
            .output();

        match result {
            Ok(out) if out.status.success() => {
                let _ = std::fs::remove_file(&ps_path);
                let node_exe = find_node_exe(&root);
                match node_exe {
                    Some(dir) => {
                        *app.state::<AppState>().node_dir.lock().unwrap() = Some(dir.clone());
                        emit(&app, "installed", "Node.js 安装完成，正在启动 DSH…", None);
                        boot(app);
                    }
                    None => {
                        let err = String::from_utf8_lossy(&out.stdout);
                        emit(
                            &app,
                            "error",
                            "Node.js 安装位置异常",
                            Some(format!("未找到 node.exe。输出：{err}")),
                        );
                    }
                }
            }
            Ok(out) => {
                let err = String::from_utf8_lossy(&out.stderr);
                let tail: String = err.chars().rev().take(600).collect::<Vec<_>>().into_iter().rev().collect();
                emit(&app, "error", "Node.js 安装失败", Some(tail));
            }
            Err(e) => emit(&app, "error", "Node.js 安装失败", Some(e.to_string())),
        }
    });
}

fn find_node_exe(root: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() && p.join("node.exe").is_file() {
            return Some(p);
        }
    }
    None
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct QuickAskEvent {
    kind: &'static str,
    text: String,
    exit_code: Option<i32>,
}

fn emit_quick_ask(app: &AppHandle, kind: &'static str, text: String, exit_code: Option<i32>) {
    let _ = app.emit(
        "quick-ask-event",
        QuickAskEvent {
            kind,
            text,
            exit_code,
        },
    );
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    let _ = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show();
    debug_log(&format!("notification: {title} - {body}"));
}

/// Run a one-shot headless dsh task (quick-ask): stream output to the popup
/// and send a system notification when done.
fn run_quick_ask_task(app: AppHandle, task: String) {
    debug_log(&format!("quick_ask: task received: {task}"));
    if app
        .state::<AppState>()
        .quick_ask_child
        .lock()
        .unwrap()
        .is_some()
    {
        emit_quick_ask(&app, "error", "已有任务在运行".to_string(), None);
        return;
    }

    let state = app.state::<AppState>();
    let node_dir = state.node_dir.lock().unwrap().clone();

    let mut cmd = Command::new("cmd");
    cmd.arg("/C");
    // Prefer a globally installed `dsh` over npx (see spawn_dsh for why).
    let use_global = find_global_dsh();
    if use_global {
        debug_log("quick_ask: using globally installed `dsh`");
        cmd.arg("dsh");
    } else if let Some(dir) = &node_dir {
        cmd.arg(dir.join("npx.cmd"));
        let path = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", format!("{};{}", dir.display(), path));
        cmd.arg("--yes").arg("@deepseek-ai/dsh");
    } else {
        cmd.arg("npx").arg("--yes").arg("@deepseek-ai/dsh");
    }
    cmd.env("npm_config_allow_scripts", NPM_ALLOW_SCRIPTS);
    cmd.arg("--profile")
        .arg("headless")
        .arg(&task)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            emit_quick_ask(&app, "error", format!("无法启动任务：{e}"), None);
            return;
        }
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    *app.state::<AppState>().quick_ask_child.lock().unwrap() = Some(child);

    let app_io = app.clone();
    let reader = std::thread::spawn(move || {
        let mut full = String::new();
        if let Some(out) = stdout {
            let mut reader = BufReader::new(out);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        full.push_str(&line);
                        emit_quick_ask(&app_io, "output", line.clone(), None);
                    }
                }
            }
        }
        if let Some(mut err) = stderr {
            let mut s = String::new();
            let _ = err.read_to_string(&mut s);
            if !s.is_empty() {
                full.push_str(&s);
                emit_quick_ask(&app_io, "output", s, None);
            }
        }
        full
    });

    let app_waiter = app.clone();
    std::thread::spawn(move || {
        // Take the child out of state (clears the "busy" flag) and wait on it.
        let mut child = app_waiter
            .state::<AppState>()
            .quick_ask_child
            .lock()
            .unwrap()
            .take()
            .expect("quick_ask_child must be Some");
        let status = child.wait();
        let full = reader.join().unwrap_or_default();
        debug_log(&format!("quick_ask: task finished, full output: {full}"));
        let code = status.as_ref().ok().and_then(|s| s.code());
        match (&status, code) {
            (Ok(s), _) if s.success() => {
                notify(&app_waiter, "DSH 快问完成", &task_summary(&task, &full));
                emit_quick_ask(&app_waiter, "done", full, None);
            }
            (Ok(s), _) => {
                emit_quick_ask(&app_waiter, "error", full, s.code());
                notify(&app_waiter, "DSH 快问失败", &format!("退出码：{:?}", s.code()));
            }
            (Err(e), _) => {
                emit_quick_ask(&app_waiter, "error", format!("任务异常：{e}"), None);
            }
        }
    });
}

fn task_summary(task: &str, output: &str) -> String {
    let task_short: String = task.chars().take(40).collect();
    let out_trimmed = output.trim();
    let out_short: String = out_trimmed.chars().take(120).collect();
    if out_short.is_empty() {
        format!("任务「{task_short}」已完成")
    } else {
        format!("任务「{task_short}」→ {out_short}")
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[tauri::command]
fn start_boot(app: AppHandle) {
    boot(app);
}

#[tauri::command]
fn install_node(app: AppHandle) {
    install_portable_node(app);
}

#[tauri::command]
fn quick_ask(app: AppHandle, task: String) {
    run_quick_ask_task(app, task);
}

#[tauri::command]
fn hide_quick_ask(app: AppHandle) {
    if let Some(w) = app.get_webview_window("quick-ask") {
        let _ = w.hide();
    }
}

/// Called by the quick-ask page when it is ready; focuses the input.
#[tauri::command]
fn quick_ask_ready(app: AppHandle) {
    if let Some(w) = app.get_webview_window("quick-ask") {
        let _ = w.set_focus();
    }
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let autostart = app.autolaunch();
    if enabled {
        autostart.enable().map_err(|e| e.to_string())?;
    } else {
        autostart.disable().map_err(|e| e.to_string())?;
    }
    Ok(autostart.is_enabled().unwrap_or(enabled))
}

#[tauri::command]
fn is_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn get_shortcut(app: AppHandle) -> String {
    load_shortcut(&app)
}

#[tauri::command]
fn set_shortcut(app: AppHandle, shortcut: String) -> Result<String, String> {
    let shortcut = shortcut.trim().to_string();
    if shortcut.is_empty() {
        return Err("快捷键不能为空".into());
    }
    // Validate by attempting to register it.
    let app2 = app.clone();
    app2.global_shortcut()
        .register(shortcut.as_str())
        .map_err(|e| format!("快捷键无效或已被占用：{e}"))?;
    let _ = app2.global_shortcut().unregister(shortcut.as_str());
    save_shortcut(&app, &shortcut)?;
    debug_log(&format!("shortcut saved as {shortcut}"));
    Ok(shortcut)
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.state::<AppState>().quitting.store(true, Ordering::SeqCst);
    kill_spawned_child(&app);
    app.exit(0);
}

fn check_update(app: &AppHandle) {
    use tauri_plugin_updater::UpdaterExt;
    let app = app.clone();
    std::thread::spawn(move || {
        notify(&app, "检查更新", "正在检查新版本…");
        let updater = match app.updater() {
            Ok(u) => u,
            Err(e) => {
                notify(&app, "检查更新失败", &format!("{e}"));
                return;
            }
        };
        match tauri::async_runtime::block_on(updater.check()) {
            Ok(Some(update)) => {
                let version = update.version.clone();
                notify(
                    &app,
                    "发现新版本",
                    &format!("DSH Desktop {version} 可用，正在下载…"),
                );
                match tauri::async_runtime::block_on(update.download_and_install(
                    |_chunk, _total| {},
                    || {},
                )) {
                    Ok(_) => {
                        // On Windows the updater runs the NSIS installer
                        // (`/UPDATE`, passive mode) and exits this process
                        // itself inside download_and_install, so this branch
                        // is effectively unreachable here — the installer
                        // relaunches the app. For other platforms, exit
                        // cleanly after killing the spawned dsh child.
                        debug_log("update: installed, exiting");
                        kill_spawned_child(&app);
                        app.state::<AppState>().quitting.store(true, Ordering::SeqCst);
                        app.exit(0);
                    }
                    Err(e) => {
                        notify(&app, "更新失败", &format!("下载安装失败：{e}"));
                    }
                }
            }
            Ok(None) => {
                notify(&app, "已是最新", "当前已是最新版本。");
            }
            Err(e) => {
                notify(&app, "检查更新失败", &format!("{e}"));
            }
        }
    });
}

fn toggle_quick_ask(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("quick-ask") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            let _ = w.show();
            let _ = w.unminimize();
            let _ = w.set_focus();
        }
    }
}

fn shortcut_config_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("settings.json"))
}

fn load_shortcut(app: &AppHandle) -> String {
    if let Some(path) = shortcut_config_path(app) {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(s) = v.get("quick_ask_shortcut").and_then(|x| x.as_str()) {
                    if !s.trim().is_empty() {
                        return s.to_string();
                    }
                }
            }
        }
    }
    QUICK_ASK_SHORTCUT.to_string()
}

fn save_shortcut(app: &AppHandle, shortcut: &str) -> Result<(), String> {
    let path = shortcut_config_path(app).ok_or("无法获取配置目录")?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut map = serde_json::Map::new();
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(serde_json::Value::Object(existing)) = serde_json::from_str(&content) {
            map = existing;
        }
    }
    map.insert(
        "quick_ask_shortcut".into(),
        serde_json::Value::String(shortcut.to_string()),
    );
    std::fs::write(&path, serde_json::to_string_pretty(&serde_json::Value::Object(map)).unwrap())
        .map_err(|e| e.to_string())
}

fn setup_global_shortcut(app: &AppHandle) {
    let shortcut = load_shortcut(app);
    let app = app.clone();
    let result = app.global_shortcut().on_shortcut(shortcut.as_str(), move |app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            toggle_quick_ask(app);
        }
    });
    if let Err(e) = result {
        debug_log(&format!(
            "register {shortcut} failed ({e}), trying fallback {QUICK_ASK_SHORTCUT_FALLBACK}"
        ));
        let app2 = app.clone();
        let _ = app2.global_shortcut().on_shortcut(QUICK_ASK_SHORTCUT_FALLBACK, move |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                toggle_quick_ask(app);
            }
        });
    } else {
        debug_log(&format!("registered global shortcut {shortcut}"));
    }
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::CheckMenuItem;

    let show = MenuItem::with_id(app, "show", "打开主窗口", true, None::<&str>)?;
    let quick = MenuItem::with_id(app, "quick", "快问（Alt+Space）", true, None::<&str>)?;
    let update = MenuItem::with_id(app, "update", "检查更新", true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "开机自启",
        true,
        app.autolaunch().is_enabled().unwrap_or(false),
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quick, &update, &autostart, &quit])?;

    let mut builder = TrayIconBuilder::with_id("dsh-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main_window(app),
            "quick" => toggle_quick_ask(app),
            "update" => check_update(app),
            "autostart" => {
                let enabled = app.autolaunch().is_enabled().unwrap_or(false);
                if enabled {
                    let _ = app.autolaunch().disable();
                } else {
                    let _ = app.autolaunch().enable();
                }
                debug_log(&format!(
                    "autostart toggled to {}",
                    app.autolaunch().is_enabled().unwrap_or(false)
                ));
            }
            "quit" => {
                app.state::<AppState>()
                    .quitting
                    .store(true, Ordering::SeqCst);
                kill_spawned_child(app);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(AppState {
            child: Mutex::new(None),
            boot_log: Arc::new(Mutex::new(String::new())),
            node_dir: Mutex::new(None),
            quick_ask_child: Mutex::new(None),
            quitting: AtomicBool::new(false),
        })
        .setup(|app| {
            setup_tray(app.handle())?;
            setup_global_shortcut(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_boot,
            install_node,
            quick_ask,
            hide_quick_ask,
            quick_ask_ready,
            set_autostart,
            is_autostart,
            get_shortcut,
            set_shortcut,
            quit_app
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let quitting = window
                    .app_handle()
                    .state::<AppState>()
                    .quitting
                    .load(Ordering::SeqCst);
                if !quitting {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

