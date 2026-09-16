# DSH Desktop

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

DeepSeek Harness (DSH) 的 Windows 桌面客户端。

核心原则：

- **不打包 dsh 本体 / Web UI**（界面仍是 dsh 自己的 Web UI）
- **钉死本地运行时**：私有 Node + 私有 npm prefix 中的 `@deepseek-ai/dsh`
- **稳定可开 + 手动升级**（启动热路径 **禁止 npx**）
- **开机自启 = 完全静默托盘**（失败只写诊断 / 通知，不弹致命窗）

## 功能（v0.2）

- 双击 exe：检测已有服务 → 否则用钉死路径启动 `dsh web --no-open`
- **与 CLI 共用 `~/.dsh`**：桌面端通过 `DSH_HOME` 指向真实用户目录，会话 / 设置 / 凭证与命令行 dsh 完全互通
- **运行时自动跟进**：发现钉死运行时里的 dsh 落后于本壳适配版本时，正常启动会自动升级一次（静默自启不做网络操作）
- **启动自愈**：拉起服务前先 `dsh --dump-config` 体检 `profiles/web`，无法启动的旧 profile 自动备份并重建；服务就绪后持续监视子进程，异常退出回到启动页而不是黑屏；任何配置重建都会同步清空 WebView 缓存（服务端/客户端同代更新）
- **界面故障自愈**：壳向页面注入看门脚本，捕获"插件加载失败"这类不崩溃的客户端故障，自动执行一次完整重置（同次运行最多 2 次）；启动时检测 WebView2 运行时版本，过旧会提示升级
- **一键重置**：托盘「重置 DSH 配置（修复界面异常）」= 停服务 + 备份重建配置 + 清界面缓存 + 重启，处理自动修复覆盖不到的异常
- 首次运行向导：安装便携 Node，并把 dsh 装到应用数据目录的 `runtime/`
- 系统托盘：打开主窗口 / 升级 DSH / 重置 DSH 配置 / 检查壳更新 / 诊断 / 开机自启 / 退出
- 开机自启：带 `--minimized`，仅托盘，短延迟后再拉服务
- 壳自动更新：Tauri updater（与 dsh 版本解耦）
- 适配 dsh **0.1.6+ 的 Web 鉴权**：解析 dsh 输出的带 token 地址打开界面（健康检查兼容 401）

## 明确不做（v1）

- 启动时 `npx` 拉最新
- 另起 `dsh --profile headless` 的「快问」
- 自研替换 dsh Web UI

## 版本策略

壳从不复制 dsh 界面。壳里钉死一个**明确适配的 dsh 版本**（`install.rs` 的 `DSH_NPM_SPEC`，当前 `0.1.6-alpha.1`）：

- 首次安装向导、托盘「升级 DSH（手动）」都装这个精确版本
- 正常启动发现已装版本**落后**于钉死版本时，自动升级一次再启动（失败则用现有版本兜底）
- 已装版本**更新**于钉死版本时不会降级
- 官方发新版后，由新壳版本带动升级；启动热路径始终不走 npx

## 用户使用

1. 下载 Release 中的安装包 / exe
2. 双击运行；若提示需要运行时，点「一键安装运行时」
3. （可选）托盘勾选「开机自启（静默托盘）」

## 数据目录

**DSH 数据（与 CLI 共用）**

```
%USERPROFILE%/.dsh/
  sessions/  storages/  settings.yaml  .credentials.yaml  profiles/ …
```

> 旧版桌面壳曾把数据隔离在应用目录的 `dsh-home/` 里；新版首次启动会把它一次性合并进 `~/.dsh`（只补缺，不覆盖），之后不再使用。

**桌面壳自身**

```
%APPDATA%/com.dsh.desktop/       # Tauri appDataDir（Roaming）
  settings.json                  # 钉死的 nodeExe / dshPath / 版本等
  runtime/node/                  # 便携 Node
  runtime/npm-prefix/            # npm global prefix（dsh.cmd 在此）
  dsh-home/                      # 旧版隔离目录（已合并则不再使用）
  logs/boot-YYYYMMDD.log
  state/last-error.json          # 自启失败摘要
```

## 开发

环境：Windows 10/11、Node.js、Rust

```bash
npm install
npm run tauri dev
npm run tauri build
```

### 调试环境变量

| 变量 | 作用 |
|---|---|
| `DSH_DESKTOP_PORT` | 覆盖端口（默认 3080，不写入 settings） |
| `DSH_DESKTOP_LOG_DIR` | 额外文件日志目录 |

### 自启调试

```bash
# 模拟开机静默
dsh-desktop.exe --minimized
```

## 架构

```
┌─────────────────────────────────────────────┐
│  DSH Desktop (Tauri 2)                      │
│  settings 绝对路径 → spawn 钉死 dsh         │
│  健康检查：TCP + HTTP 状态（不嗅 doctype）   │
│  自启：--minimized → 仅托盘                 │
└────────────────────┬────────────────────────┘
                     │
        ┌────────────▼────────────┐
        │  dsh（私有 npm-prefix） │
        │  http://127.0.0.1:3080  │
        └─────────────────────────┘
```

## 常见问题

### 开机自启没有窗口 / 只有托盘

这是预期行为。点托盘图标打开主窗口。若服务未起来，托盘通知 + `state/last-error.json`。

### 以前依赖全局 `npm install -g` / npx

v1 改为应用目录钉死。请打开一次应用走安装向导；或托盘「升级 DSH」写入私有 prefix。

### npm 安装 dsh 需要原生模块脚本

安装/升级时会传入 allow-scripts 列表（node-pty、koffi 等）。若仍失败，打开诊断页查看输出尾部。

## 许可证

[MIT](LICENSE) © 2026 蒙 寸尘 (HsgtLgt)
