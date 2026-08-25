# DSH Desktop

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

DeepSeek Harness (DSH) 的 Windows 桌面客户端。

核心原则：

- **不打包 dsh 本体 / Web UI**（界面仍是 dsh 自己的 Web UI）
- **钉死本地运行时**：私有 Node + 私有 npm prefix 中的 `@deepseek-ai/dsh`
- **稳定可开 + 手动升级**（启动热路径 **禁止 npx**）
- **开机自启 = 完全静默托盘**（失败只写诊断 / 通知，不弹致命窗）

## 功能（v1）

- 双击 exe：检测已有服务 → 否则用钉死路径启动 `dsh web --no-open`
- 首次运行向导：安装便携 Node，并把 dsh 装到 `%LOCALAPPDATA%/dsh-desktop/runtime/`
- 系统托盘：打开主窗口 / 升级 DSH / 检查壳更新 / 诊断 / 开机自启 / 退出
- 开机自启：带 `--minimized`，仅托盘，短延迟后再拉服务
- 壳自动更新：Tauri updater（与 dsh 版本解耦）

## 明确不做（v1）

- 启动时 `npx` 拉最新
- 另起 `dsh --profile headless` 的「快问」
- 自研替换 dsh Web UI

## 为什么「永远兼容」但不「每次最新」

壳从不复制 dsh 界面。dsh 升级由托盘 **「升级 DSH（手动）」** 触发，装进同一私有 prefix 后重启服务。  
下次冷启动仍走同一绝对路径，不依赖登录态 PATH，也不走 npx。

## 用户使用

1. 下载 Release 中的安装包 / exe
2. 双击运行；若提示需要运行时，点「一键安装运行时」
3. （可选）托盘勾选「开机自启（静默托盘）」

## 数据目录

```
%LOCALAPPDATA%/dsh-desktop/
  settings.json          # 钉死的 nodeExe / dshPath / 版本等
  runtime/
    node/                # 便携 Node
    npm-prefix/          # npm global prefix（dsh.cmd 在此）
  logs/boot-YYYYMMDD.log
  state/last-error.json  # 自启失败摘要
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
