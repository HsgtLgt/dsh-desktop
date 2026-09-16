mod install;
mod migrate;
mod paths;
mod profile_repair;
mod probe;
mod settings;

use std::{
    io::Read,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Url, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_notification::NotificationExt;

use install::{
    install_runtime, recover_from_disk, should_upgrade_dsh, upgrade_dsh, desired_dsh_version,
    NPM_ALLOW_SCRIPTS,
};
use paths::AppPaths;
use profile_repair::{ensure_web_profile, user_dsh_home};
use probe::{probe, probe_status};
use settings::Settings;

const MAX_BOOT_LOG: usize = 8192;
const MAX_AUTO_RESTART: u8 = 2;
const AUTOSTART_DELAY: Duration = Duration::from_secs(5);

struct AppState {
    child: Mutex<Option<Child>>,
    boot_log: Arc<Mutex<String>>,
    settings: Mutex<Settings>,
    paths: Mutex<Option<AppPaths>>,
    splash_url: Mutex<Option<Url>>,
    /// Last boot-status event — frontend may miss live emits during splash load.
    last_boot: Mutex<Option<BootEvent>>,
    silent: AtomicBool,
    booting: AtomicBool,
    quitting: AtomicBool,
    restart_count: Mutex<u8>,
    /// Incremented on every spawn attempt. Post-ready watchers only report
    /// failures for the generation they started with — otherwise an
    /// upgrade/reset's intentional kill would surface as a ghost "DSH 已退出"
    /// error, and stale watcher threads would pile up.
    spawn_generation: AtomicU64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BootEvent {
    stage: String,
    message: String,
    detail: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LastError {
    at: String,
    stage: String,
    message: String,
    detail: Option<String>,
}

fn emit(app: &AppHandle, stage: &str, message: &str, detail: Option<String>) {
    let event = BootEvent {
        stage: stage.into(),
        message: message.into(),
        detail,
    };
    if let Ok(mut guard) = app.state::<AppState>().last_boot.lock() {
        *guard = Some(event.clone());
    }
    debug_log(&format!("boot-status: {} — {}", event.stage, event.message));
    let _ = app.emit("boot-status", event);
}

fn debug_log(msg: &str) {
    if let Some(dir) = std::env::var_os("DSH_DESKTOP_LOG_DIR") {
        let path = std::path::Path::new(&dir).join("dsh-desktop.log");
        use std::io::Write as _;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(f, "{msg}");
        }
    }
    // Also append to app boot log file when paths known — best-effort via env only here.
    eprintln!("[dsh-desktop] {msg}");
}

fn append_boot_file(app: &AppHandle, msg: &str) {
    let paths = {
        let state = app.state::<AppState>();
        let guard = state.paths.lock().unwrap();
        guard.clone()
    };
    let Some(paths) = paths else {
        return;
    };
    let path = paths.boot_log_file();
    use std::io::Write as _;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(f, "{msg}");
    }
}

fn write_last_error(app: &AppHandle, stage: &str, message: &str, detail: Option<String>) {
    let paths = {
        let state = app.state::<AppState>();
        let guard = state.paths.lock().unwrap();
        guard.clone()
    };
    let Some(paths) = paths else {
        return;
    };
    let err = LastError {
        at: format!("{:?}", std::time::SystemTime::now()),
        stage: stage.into(),
        message: message.into(),
        detail,
    };
    if let Ok(json) = serde_json::to_string_pretty(&err) {
        let _ = std::fs::write(paths.last_error_file(), json);
    }
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

fn is_minimized_arg() -> bool {
    std::env::args().any(|a| a == "--minimized" || a == "-minimized")
}

fn effective_port(settings: &Settings) -> u16 {
    std::env::var("DSH_DESKTOP_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(settings.port)
}

fn tee_into_log<R: Read + Send + 'static>(mut reader: R, log: Arc<Mutex<String>>) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let s = String::from_utf8_lossy(&buf[..n]).to_string();
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

fn load_or_recover_settings(app: &AppHandle) -> Result<(AppPaths, Settings), String> {
    debug_log("load_or_recover: begin");
    let paths = AppPaths::from_app(app)?;
    debug_log(&format!("load_or_recover: root={}", paths.root.display()));
    paths.ensure()?;

    // One-time: fold the old isolated dsh-home/ into the real ~/.dsh.
    if let Some(user_home) = user_dsh_home() {
        let marker = paths.state_dir().join(".legacy-home-merged");
        if !marker.is_file() {
            match migrate::merge_legacy_desktop_home(&paths.legacy_dsh_home(), &user_home) {
                Ok(Some(summary)) => {
                    debug_log(&format!("legacy merge: {summary}"));
                    emit(
                        app,
                        "detecting",
                        "已合并旧桌面数据目录到 ~/.dsh…",
                        Some(summary),
                    );
                }
                Ok(None) => debug_log("legacy merge: nothing to merge"),
                Err(e) => debug_log(&format!("legacy merge failed (continuing): {e}")),
            }
            let _ = std::fs::write(&marker, "done\n");
        }
    }

    let mut settings = Settings::load(&paths.settings_file());

    if !settings.runtime_ready() {
        if let Some((node_exe, dsh_path, ver)) = recover_from_disk(&paths) {
            settings.node_exe = Some(node_exe.display().to_string());
            settings.dsh_path = Some(dsh_path.display().to_string());
            settings.dsh_version = ver;
            let _ = settings.save(&paths.settings_file());
            debug_log("recovered runtime paths from disk into settings.json");
        }
    }

    debug_log(&format!(
        "load_or_recover: runtime_ready={}",
        settings.runtime_ready()
    ));
    Ok((paths, settings))
}

fn persist_settings(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let state = app.state::<AppState>();
    let paths = state
        .paths
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "paths 未初始化".to_string())?;
    settings.save(&paths.settings_file())?;
    *state.settings.lock().unwrap() = settings.clone();
    Ok(())
}

/// Spawn pinned dsh web. Never uses npx on the production hot path.
///
/// `dsh_home` is the user's real `~/.dsh` so desktop and CLI share sessions,
/// settings and credentials (passed via DSH_HOME, same env var the CLI reads).
fn spawn_dsh(
    settings: &Settings,
    dsh_home: &Path,
    boot_log: Arc<Mutex<String>>,
) -> std::io::Result<Child> {
    let port = effective_port(settings);
    let node_exe = settings
        .node_exe_path()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "missing nodeExe"))?;
    let dsh_path = settings
        .dsh_path_buf()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "missing dshPath"))?;
    let node_dir = node_exe.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "nodeExe has no parent")
    })?;

    std::fs::create_dir_all(dsh_home)?;

    let mut cmd = Command::new("cmd");
    cmd.arg("/C").arg(&dsh_path).arg("web").arg("--port").arg(port.to_string());
    // Prefer --no-open so the shell owns the window.
    cmd.arg("--no-open");

    let path = std::env::var("PATH").unwrap_or_default();
    // npm-prefix (parent of dsh.cmd) + node dir first
    let prefix = dsh_path
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    cmd.env(
        "PATH",
        format!("{};{};{}", node_dir.display(), prefix, path),
    );
    // Same home as CLI dsh (`~/.dsh`) — sessions/settings stay in one place.
    cmd.env("DSH_HOME", dsh_home);
    // dsh uses cwd() as the default workspace root — keep the user profile,
    // avoids System32 when launched from autostart.
    if let Some(home) = std::env::var_os("USERPROFILE") {
        cmd.current_dir(home);
    }
    cmd.env("npm_config_allow_scripts", NPM_ALLOW_SCRIPTS);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }

    debug_log(&format!(
        "spawn_dsh: {} web --port {port} --no-open (node={}, DSH_HOME={})",
        dsh_path.display(),
        node_exe.display(),
        dsh_home.display()
    ));

    let mut child = cmd.spawn()?;
    if let Some(out) = child.stdout.take() {
        tee_into_log(out, boot_log.clone());
    }
    if let Some(err) = child.stderr.take() {
        tee_into_log(err, boot_log);
    }
    Ok(child)
}

/// Parse the last `dsh web: <url>` line from the boot log.
///
/// Since dsh 0.1.6 the printed URL carries a per-launch auth token; without it
/// the root only answers 401. The token exists solely in child stdout, so this
/// is the only way to obtain the authenticated URL. Older dsh versions print
/// the plain URL, which still navigates fine.
fn extract_announced_url(app: &AppHandle, port: u16) -> Option<String> {
    let state = app.state::<AppState>();
    let log = state.boot_log.lock().unwrap();
    let needle = "dsh web: ";
    let mut found: Option<String> = None;
    let mut from = 0;
    while let Some(pos) = log[from..].find(needle) {
        let abs = from + pos + needle.len();
        let rest = &log[abs..];
        let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
        let url = &rest[..end];
        let ok = url.starts_with("http://")
            && (url.contains(&format!("127.0.0.1:{port}")) || url.contains(&format!("localhost:{port}")));
        if ok {
            found = Some(url.to_string());
        }
        from = abs;
    }
    found
}

fn kill_spawned_child(app: &AppHandle) {
    let state = app.state::<AppState>();
    // Invalidate watchers BEFORE killing: an intentional stop must never look
    // like a crash to the post-ready watcher, including the window between
    // the kill and the next spawn bumping the generation itself.
    state.spawn_generation.fetch_add(1, Ordering::SeqCst);
    let child = {
        let mut guard = state.child.lock().unwrap();
        guard.take()
    };
    if let Some(mut child) = child {
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/PID", &child.id().to_string(), "/T", "/F"])
                .status();
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn navigate_to_dsh(app: &AppHandle, port: u16) {
    // Prefer the URL dsh itself announced (carries the 0.1.6+ auth token);
    // fall back to the plain loopback URL for older versions.
    let url = extract_announced_url(app, port)
        .unwrap_or_else(|| format!("http://127.0.0.1:{port}"));
    if let Some(w) = app.get_webview_window("main") {
        let res = w.navigate(Url::parse(&url).expect("valid URL"));
        debug_log(&format!("navigate to {url}: {:?}", res.map(|_| "ok")));
    }
}

fn navigate_to_splash(app: &AppHandle) {
    let splash = app.state::<AppState>().splash_url.lock().unwrap().clone();
    if let (Some(w), Some(url)) = (app.get_webview_window("main"), splash) {
        let res = w.navigate(url);
        debug_log(&format!("navigate to splash: {:?}", res.map(|_| "ok")));
    }
}

/// Drop every cached web resource (HTTP cache, service worker, cookies) of the
/// shell's webview. Required whenever the dsh profile tree is rebuilt: the
/// client loads UI plugin chunks through the WebView cache, and stale chunks
/// from a previous broken state can fail to import even though the server now
/// serves a fresh build. The dsh auth cookie is wiped too — re-minted on the
/// next token-URL navigation, so nothing is lost.
fn clear_webview_cache(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        match w.clear_all_browsing_data() {
            Ok(_) => debug_log("webview browsing data cleared"),
            Err(e) => debug_log(&format!("clear browsing data failed: {e}")),
        }
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        // Do not force focus — avoids stealing mouse/keyboard from the user.
    }
}

fn hide_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}

fn fail(app: &AppHandle, silent: bool, stage: &str, message: &str, detail: Option<String>) {
    write_last_error(app, stage, message, detail.clone());
    append_boot_file(app, &format!("FAIL [{stage}] {message} {:?}", detail));
    // Cache status BEFORE navigating away from a dead dsh page, so splash can sync.
    if silent {
        notify(app, "DSH Desktop", message);
        // Do not emit error UI stages that would demand a modal — still emit for diagnostics page if opened later
        emit(app, "error-silent", message, detail);
    } else {
        emit(app, "error", message, detail);
    }
    navigate_to_splash(app);
    if !silent {
        show_main_window(app);
    }
}

/// After Ready, keep watching the child. dsh may print its URL before plugins
/// finish loading and then exit — without this the WebView would just go black.
/// Exits silently when a newer generation spawns (upgrade/reset) or on quit.
fn watch_child_after_ready(app: AppHandle, silent: bool, generation: u64) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(800));
        let state = app.state::<AppState>();
        if state.quitting.load(Ordering::SeqCst)
            || state.spawn_generation.load(Ordering::SeqCst) != generation
        {
            return;
        }
        let died = state
            .child
            .lock()
            .unwrap()
            .as_mut()
            .and_then(|c| c.try_wait().ok().flatten());
        if let Some(status) = died {
            let tail = boot_log_tail(&app);
            let detail = if tail.is_empty() {
                format!("退出码：{status}")
            } else {
                format!("退出码：{status}\n\n--- dsh 输出尾部 ---\n{tail}")
            };
            fail(&app, silent, "exited", "DSH 进程已退出", Some(detail));
            return;
        }
    });
}

fn boot(app: AppHandle) {
    let state = app.state::<AppState>();
    if state.booting.swap(true, Ordering::SeqCst) {
        debug_log("boot: already in progress, skip");
        return;
    }
    std::thread::spawn(move || {
        let silent = app.state::<AppState>().silent.load(Ordering::SeqCst);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            boot_inner(&app, silent);
        }));
        app.state::<AppState>()
            .booting
            .store(false, Ordering::SeqCst);
        if result.is_err() {
            fail(
                &app,
                silent,
                "panic",
                "启动过程发生内部错误",
                Some("详见日志".into()),
            );
        }
    });
}

fn boot_inner(app: &AppHandle, silent: bool) {
    debug_log(&format!("boot_inner: silent={silent}"));
    if silent {
        std::thread::sleep(AUTOSTART_DELAY);
    }

    let (paths, settings) = match load_or_recover_settings(app) {
        Ok(v) => v,
        Err(e) => {
            fail(app, silent, "resolving", "初始化应用目录失败", Some(e));
            return;
        }
    };
    *app.state::<AppState>().paths.lock().unwrap() = Some(paths.clone());
    *app.state::<AppState>().settings.lock().unwrap() = settings.clone();

    let port = effective_port(&settings);
    emit(app, "detecting", "正在检测 DSH 服务…", None);

    if probe(port) {
        debug_log("boot: existing healthy service on port");
        // A service we did not spawn keeps its launch token to itself; the
        // webview can still get in via its persisted cookie. Say so, instead
        // of letting a bare 401 page confuse the user when there is no cookie.
        if probe_status(port) == Some(401) {
            emit(
                app,
                "detecting",
                "检测到已有 DSH 服务（带访问令牌），正在用已保存的凭据打开…",
                Some("若打开后显示 401，请在该服务的终端里复制带 token 的地址，或重启 DSH Desktop 让壳自己拉起服务。".into()),
            );
        } else {
            emit(app, "ready", "DSH 已在运行，正在打开…", None);
        }
        navigate_to_dsh(app, port);
        if !silent {
            show_main_window(app);
        }
        return;
    }

    if !settings.runtime_ready() {
        if silent {
            fail(
                app,
                true,
                "need-install",
                "运行时未就绪，请手动打开一次完成安装",
                Some("开机自启不会弹出安装向导。".into()),
            );
            return;
        }
        emit(
            app,
            "need-runtime",
            "需要安装本地运行时",
            Some("将安装便携版 Node.js，并把 DeepSeek Harness 钉死到应用目录（不走 npx）。".into()),
        );
        return;
    }

    emit(app, "starting", "正在启动 DSH…", None);

    let dsh_home = match user_dsh_home() {
        Some(h) => h,
        None => {
            fail(
                app,
                silent,
                "spawning",
                "启动 DSH 失败",
                Some("无法解析用户目录（USERPROFILE）".into()),
            );
            return;
        }
    };

    // Keep the pinned runtime at the version this shell build was validated
    // against (e.g. 0.1.1-rc.2 → 0.1.6). Skipped in silent autostart so boot
    // never touches the network in the background; the next manual open does it.
    if !silent && should_upgrade_dsh(&paths.npm_prefix()) {
        emit(
            app,
            "installing",
            &format!(
                "检测到 DSH 版本落后，正在升级到 {}…（首次约需 1-2 分钟）",
                desired_dsh_version()
            ),
            None,
        );
        append_boot_file(app, &format!("auto-upgrade: target {}", desired_dsh_version()));
        match upgrade_dsh(&settings, &paths) {
            Ok(result) => {
                let mut s = settings.clone();
                s.dsh_path = Some(result.dsh_path.display().to_string());
                s.dsh_version = result.dsh_version.clone();
                if let Err(e) = persist_settings(app, &s) {
                    debug_log(&format!("auto-upgrade: settings persist failed: {e}"));
                } else {
                    debug_log(&format!(
                        "auto-upgrade: now {}",
                        result.dsh_version.unwrap_or_default()
                    ));
                }
            }
            Err(e) => {
                // Fall back to booting whatever is installed; retry next open.
                debug_log(&format!("auto-upgrade failed (continuing): {e}"));
                notify(
                    app,
                    "DSH 升级失败",
                    &format!("将先用现有版本启动，下次打开时重试。{e}"),
                );
            }
        }
    }

    // Keep the recorded version honest for the diagnostics page — it can go
    // stale when the prefix was upgraded outside the shell's own flow.
    let on_disk = install::read_dsh_version(&paths.npm_prefix());
    if on_disk != settings.dsh_version {
        let mut s = settings.clone();
        s.dsh_version = on_disk;
        if let Err(e) = persist_settings(app, &s) {
            debug_log(&format!("dsh_version sync failed: {e}"));
        } else {
            debug_log(&format!("dsh_version synced to {:?}", s.dsh_version));
        }
    }

    if settings.runtime_ready() {
        match ensure_web_profile(&settings, &dsh_home) {
            Ok(Some(msg)) => {
                debug_log(&format!("profile repair: {msg}"));
                emit(app, "starting", "正在修复 DSH 配置…", Some(msg));
                // The client cache belongs to the old profile build — drop it.
                clear_webview_cache(app);
            }
            Ok(None) => {}
            Err(e) => debug_log(&format!("profile repair failed (continuing): {e}")),
        }
    }

    start_dsh_and_wait(app, &settings, &dsh_home, silent);
}

enum WaitOutcome {
    Ready(u64),
    SpawnErr(String),
    Exited { detail: String },
    Timeout { detail: String },
}

fn start_dsh_and_wait(app: &AppHandle, settings: &Settings, dsh_home: &Path, silent: bool) {
    let dsh_home = dsh_home.to_path_buf();
    for attempt in 0..2u8 {
        match spawn_and_await(app, settings, &dsh_home) {
            WaitOutcome::Ready(generation) => {
                *app.state::<AppState>().restart_count.lock().unwrap() = 0;
                emit(app, "ready", "DSH 已就绪", None);
                navigate_to_dsh(app, effective_port(settings));
                if !silent {
                    show_main_window(app);
                }
                watch_child_after_ready(app.clone(), silent, generation);
                return;
            }
            WaitOutcome::SpawnErr(e) => {
                fail(
                    app,
                    silent,
                    "spawning",
                    "启动 DSH 失败",
                    Some(format!("无法启动进程：{e}")),
                );
                return;
            }
            WaitOutcome::Exited { detail } => {
                // A stale profile from another dsh version crashes the child
                // before it serves; quarantine it and retry exactly once so
                // dsh rebuilds the profile fresh.
                if attempt == 0 && profile_repair::looks_like_profile_corruption(&detail) {
                    match profile_repair::quarantine_web_profile(&dsh_home) {
                        Ok(msg) => {
                            debug_log(&format!("profile crash-repair: {msg}"));
                            emit(
                                app,
                                "starting",
                                "检测到 DSH 配置损坏，正在自动修复后重启…",
                                Some(msg),
                            );
                            clear_webview_cache(app);
                            continue;
                        }
                        Err(e) => debug_log(&format!("profile crash-repair failed: {e}")),
                    }
                }
                fail(app, silent, "exited", "DSH 进程已退出", Some(detail));
                return;
            }
            WaitOutcome::Timeout { detail } => {
                fail(app, silent, "timeout", "DSH 启动超时", Some(detail));
                return;
            }
        }
    }
}

fn spawn_and_await(
    app: &AppHandle,
    settings: &Settings,
    dsh_home: &Path,
) -> WaitOutcome {
    let port = effective_port(settings);
    let boot_log = app.state::<AppState>().boot_log.clone();
    {
        let mut log = boot_log.lock().unwrap();
        log.clear();
    }

    let generation = app
        .state::<AppState>()
        .spawn_generation
        .fetch_add(1, Ordering::SeqCst)
        + 1;

    let child = match spawn_dsh(settings, dsh_home, boot_log) {
        Ok(c) => c,
        Err(e) => return WaitOutcome::SpawnErr(e.to_string()),
    };
    *app.state::<AppState>().child.lock().unwrap() = Some(child);

    let timeout = Duration::from_millis(settings.probe_timeout_ms);
    let interval = Duration::from_millis(settings.probe_interval_ms.max(200));
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        if probe(port) {
            // dsh may accept HTTP before the plugin tree finishes loading; wait,
            // confirm the process is still alive, and re-probe before handing
            // the WebView over.
            std::thread::sleep(Duration::from_millis(1500));
            let still_alive = app
                .state::<AppState>()
                .child
                .lock()
                .unwrap()
                .as_mut()
                .map(|c| c.try_wait().ok().flatten().is_none())
                .unwrap_or(false);
            if !still_alive {
                let tail = boot_log_tail(app);
                let detail = if tail.is_empty() {
                    "服务刚就绪即退出。".to_string()
                } else {
                    format!("服务刚就绪即退出。\n\n--- dsh 输出尾部 ---\n{tail}")
                };
                return WaitOutcome::Exited { detail };
            }
            if !probe(port) {
                continue;
            }

            debug_log("start_dsh: ready (stable)");
            return WaitOutcome::Ready(generation);
        }
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
            return WaitOutcome::Exited { detail };
        }
        std::thread::sleep(interval);
    }

    let tail = boot_log_tail(app);
    let detail = if tail.is_empty() {
        "在限定时间内未能就绪。".to_string()
    } else {
        format!("在限定时间内未能就绪。\n\n--- dsh 输出尾部 ---\n{tail}")
    };
    WaitOutcome::Timeout { detail }
}

fn run_install_wizard(app: AppHandle) {
    std::thread::spawn(move || {
        let silent = app.state::<AppState>().silent.load(Ordering::SeqCst);
        if silent {
            fail(
                &app,
                true,
                "need-install",
                "自启模式下不会运行安装向导",
                None,
            );
            return;
        }

        let paths = match AppPaths::from_app(&app) {
            Ok(p) => {
                let _ = p.ensure();
                p
            }
            Err(e) => {
                fail(&app, false, "install", "获取应用目录失败", Some(e));
                return;
            }
        };
        *app.state::<AppState>().paths.lock().unwrap() = Some(paths.clone());

        let mut last_msg = String::new();
        let mut progress = |msg: &str| {
            last_msg = msg.to_string();
            emit(&app, "installing", msg, None);
            append_boot_file(&app, msg);
        };

        match install_runtime(&paths, &mut progress) {
            Ok(result) => {
                let mut settings = app.state::<AppState>().settings.lock().unwrap().clone();
                settings.node_exe = Some(result.node_exe.display().to_string());
                settings.dsh_path = Some(result.dsh_path.display().to_string());
                settings.dsh_version = result.dsh_version;
                if let Err(e) = persist_settings(&app, &settings) {
                    fail(&app, false, "install", "写入 settings 失败", Some(e));
                    return;
                }
                emit(
                    &app,
                    "installed",
                    "运行时安装完成，正在启动…",
                    settings.dsh_version.clone(),
                );
                // Reset booting flag so boot() can run
                app.state::<AppState>()
                    .booting
                    .store(false, Ordering::SeqCst);
                boot(app);
            }
            Err(e) => {
                fail(
                    &app,
                    false,
                    "install",
                    "安装运行时失败",
                    Some(if last_msg.is_empty() {
                        e
                    } else {
                        format!("{last_msg}\n{e}")
                    }),
                );
            }
        }
    });
}

fn run_upgrade_dsh(app: AppHandle) {
    std::thread::spawn(move || {
        notify(&app, "升级 DSH", "正在升级 DeepSeek Harness…");
        emit(&app, "installing", "正在升级 DSH…", None);

        // Stop current child first
        kill_spawned_child(&app);

        let (paths, settings) = match load_or_recover_settings(&app) {
            Ok(v) => v,
            Err(e) => {
                notify(&app, "升级失败", &e);
                return;
            }
        };
        *app.state::<AppState>().paths.lock().unwrap() = Some(paths.clone());

        match upgrade_dsh(&settings, &paths) {
            Ok(result) => {
                let mut s = settings;
                s.node_exe = Some(result.node_exe.display().to_string());
                s.dsh_path = Some(result.dsh_path.display().to_string());
                s.dsh_version = result.dsh_version.clone();
                if let Err(e) = persist_settings(&app, &s) {
                    notify(&app, "升级失败", &e);
                    return;
                }
                notify(
                    &app,
                    "升级完成",
                    &format!(
                        "DSH {} 已安装，正在重启服务…",
                        result.dsh_version.unwrap_or_else(|| "未知版本".into())
                    ),
                );
                app.state::<AppState>()
                    .booting
                    .store(false, Ordering::SeqCst);
                boot(app);
            }
            Err(e) => {
                notify(&app, "升级失败", &e);
                emit(&app, "error", "升级 DSH 失败", Some(e));
            }
        }
    });
}

/// One-click hammer for failures the crash detector can't see (e.g. a client
/// UI plugin failing to import): stop the service, quarantine the profile
/// tree, drop the webview cache, and boot into a guaranteed-fresh state.
fn run_reset_dsh(app: AppHandle) {
    std::thread::spawn(move || {
        notify(&app, "重置 DSH 配置", "正在停止服务并重建配置…");
        emit(&app, "installing", "正在重置 DSH 配置…", None);
        kill_spawned_child(&app);

        let (paths, _settings) = match load_or_recover_settings(&app) {
            Ok(v) => v,
            Err(e) => {
                notify(&app, "重置失败", &e);
                return;
            }
        };
        *app.state::<AppState>().paths.lock().unwrap() = Some(paths.clone());

        let dsh_home = match user_dsh_home() {
            Some(h) => h,
            None => {
                notify(&app, "重置失败", "无法解析用户目录（USERPROFILE）");
                return;
            }
        };
        match profile_repair::quarantine_web_profile(&dsh_home) {
            Ok(msg) => {
                debug_log(&format!("manual reset: {msg}"));
                notify(&app, "重置完成", &format!("{msg}，正在重启服务…"));
            }
            Err(e) => notify(&app, "重置", &format!("备份旧配置失败（继续尝试启动）：{e}")),
        }
        clear_webview_cache(&app);

        app.state::<AppState>()
            .booting
            .store(false, Ordering::SeqCst);
        boot(app);
    });
}

fn check_update_shell(app: &AppHandle) {
    use tauri_plugin_updater::UpdaterExt;
    let app = app.clone();
    std::thread::spawn(move || {
        notify(&app, "检查更新", "正在检查桌面壳新版本…");
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
                        kill_spawned_child(&app);
                        app.state::<AppState>().quitting.store(true, Ordering::SeqCst);
                        app.exit(0);
                    }
                    Err(e) => notify(&app, "更新失败", &format!("{e}")),
                }
            }
            Ok(None) => notify(&app, "已是最新", "桌面壳已是最新版本。"),
            Err(e) => notify(&app, "检查更新失败", &format!("{e}")),
        }
    });
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "打开主窗口", true, None::<&str>)?;
    let upgrade_dsh_item = MenuItem::with_id(app, "upgrade_dsh", "升级 DSH（手动）", true, None::<&str>)?;
    let reset_dsh = MenuItem::with_id(app, "reset_dsh", "重置 DSH 配置（修复界面异常）", true, None::<&str>)?;
    let update_shell = MenuItem::with_id(app, "update", "检查壳更新", true, None::<&str>)?;
    let diagnose = MenuItem::with_id(app, "diagnose", "打开诊断页", true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "开机自启（静默托盘）",
        true,
        app.autolaunch().is_enabled().unwrap_or(false),
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &upgrade_dsh_item,
            &reset_dsh,
            &update_shell,
            &diagnose,
            &autostart,
            &quit,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("dsh-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                app.state::<AppState>()
                    .silent
                    .store(false, Ordering::SeqCst);
                show_main_window(app);
            }
            "upgrade_dsh" => {
                // User is present and clicked — leave silent mode so the
                // window shows and errors surface in the UI.
                app.state::<AppState>().silent.store(false, Ordering::SeqCst);
                run_upgrade_dsh(app.clone());
            }
            "reset_dsh" => {
                app.state::<AppState>().silent.store(false, Ordering::SeqCst);
                run_reset_dsh(app.clone());
            }
            "update" => check_update_shell(app),
            "diagnose" => {
                app.state::<AppState>()
                    .silent
                    .store(false, Ordering::SeqCst);
                let splash = app.state::<AppState>().splash_url.lock().unwrap().clone();
                if let (Some(w), Some(url)) = (app.get_webview_window("main"), splash) {
                    let _ = w.navigate(url);
                }
                show_main_window(app);
                // Surface last error on splash without restarting the service
                if let Some(paths) = app.state::<AppState>().paths.lock().unwrap().clone() {
                    if let Ok(content) = std::fs::read_to_string(paths.last_error_file()) {
                        emit(
                            app,
                            "error",
                            "最近一次启动失败",
                            Some(content),
                        );
                    } else {
                        emit(
                            app,
                            "detecting",
                            "暂无失败记录。可点「重试」重新检测。",
                            None,
                        );
                    }
                }
            }
            "autostart" => {
                let enabled = app.autolaunch().is_enabled().unwrap_or(false);
                if enabled {
                    let _ = app.autolaunch().disable();
                } else {
                    let _ = app.autolaunch().enable();
                }
                let now = app.autolaunch().is_enabled().unwrap_or(false);
                let mut s = app.state::<AppState>().settings.lock().unwrap().clone();
                s.autostart = now;
                s.start_minimized = true;
                let _ = persist_settings(app, &s);
                debug_log(&format!("autostart toggled to {now}"));
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
                tray.app_handle()
                    .state::<AppState>()
                    .silent
                    .store(false, Ordering::SeqCst);
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

#[tauri::command]
fn start_boot(app: AppHandle) {
    // User-driven: leave silent mode so errors can surface
    app.state::<AppState>()
        .silent
        .store(false, Ordering::SeqCst);
    show_main_window(&app);
    boot(app);
}

#[tauri::command]
fn install_runtime_cmd(app: AppHandle) {
    run_install_wizard(app);
}

/// Back-compat alias for old frontend button name
#[tauri::command]
fn install_node(app: AppHandle) {
    run_install_wizard(app);
}

#[tauri::command]
fn upgrade_dsh_cmd(app: AppHandle) {
    run_upgrade_dsh(app);
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let autostart = app.autolaunch();
    if enabled {
        autostart.enable().map_err(|e| e.to_string())?;
    } else {
        autostart.disable().map_err(|e| e.to_string())?;
    }
    let now = autostart.is_enabled().unwrap_or(enabled);
    let mut s = app.state::<AppState>().settings.lock().unwrap().clone();
    s.autostart = now;
    s.start_minimized = true;
    persist_settings(&app, &s)?;
    Ok(now)
}

#[tauri::command]
fn is_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.state::<AppState>().quitting.store(true, Ordering::SeqCst);
    kill_spawned_child(&app);
    app.exit(0);
}

#[tauri::command]
fn get_runtime_info(app: AppHandle) -> serde_json::Value {
    let s = app.state::<AppState>().settings.lock().unwrap().clone();
    serde_json::json!({
        "nodeExe": s.node_exe,
        "dshPath": s.dsh_path,
        "dshVersion": s.dsh_version,
        "port": effective_port(&s),
        "runtimeReady": s.runtime_ready(),
    })
}

#[tauri::command]
fn get_boot_status(app: AppHandle) -> Option<BootEvent> {
    app.state::<AppState>().last_boot.lock().unwrap().clone()
}

#[tauri::command]
fn get_last_error(app: AppHandle) -> Option<serde_json::Value> {
    let paths = {
        let state = app.state::<AppState>();
        let guard = state.paths.lock().unwrap();
        guard.clone()
    };
    let paths = paths.or_else(|| AppPaths::from_app(&app).ok())?;
    let content = std::fs::read_to_string(paths.last_error_file()).ok()?;
    serde_json::from_str(&content).ok()
}

// ---- Removed quick-ask surface (v1): stub commands keep old HTML from crashing if opened ----

#[tauri::command]
fn quick_ask(_app: AppHandle, _task: String) {
    // intentionally disabled
}

#[tauri::command]
fn hide_quick_ask(app: AppHandle) {
    if let Some(w) = app.get_webview_window("quick-ask") {
        let _ = w.hide();
    }
}

#[tauri::command]
fn quick_ask_ready(_app: AppHandle) {}

#[tauri::command]
fn get_shortcut() -> String {
    String::new()
}

#[tauri::command]
fn set_shortcut(_shortcut: String) -> Result<String, String> {
    Err("快问已在 v1 禁用".into())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let silent = is_minimized_arg();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            app.state::<AppState>()
                .silent
                .store(false, Ordering::SeqCst);
            show_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            // Autostart always passes --minimized → silent tray path
            Some(vec!["--minimized".into()]),
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(AppState {
            child: Mutex::new(None),
            boot_log: Arc::new(Mutex::new(String::new())),
            settings: Mutex::new(Settings::default()),
            paths: Mutex::new(None),
            splash_url: Mutex::new(None),
            last_boot: Mutex::new(None),
            silent: AtomicBool::new(silent),
            booting: AtomicBool::new(false),
            quitting: AtomicBool::new(false),
            restart_count: Mutex::new(0),
            spawn_generation: AtomicU64::new(0),
        })
        .setup(move |app| {
            if let Some(w) = app.get_webview_window("main") {
                if let Ok(url) = w.url() {
                    *app.state::<AppState>().splash_url.lock().unwrap() = Some(url);
                }
            }
            setup_tray(app.handle())?;
            if silent {
                hide_main_window(app.handle());
            }
            // Backend owns the initial boot for both paths
            boot(app.handle().clone());
            let _ = MAX_AUTO_RESTART; // reserved for Ready→Degraded follow-up
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_boot,
            install_runtime_cmd,
            install_node,
            upgrade_dsh_cmd,
            quick_ask,
            hide_quick_ask,
            quick_ask_ready,
            set_autostart,
            is_autostart,
            get_shortcut,
            set_shortcut,
            get_runtime_info,
            get_boot_status,
            get_last_error,
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
