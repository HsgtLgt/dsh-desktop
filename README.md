# DSH Desktop

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

DeepSeek Harness (DSH) 的 Windows 桌面客户端。核心原则：**不打包 dsh 本体、永远兼容 dsh 更新**。

## ✨ 功能

- **双击 exe 免命令**：自动检测 dsh 服务是否在运行，没运行就自动拉起
- **自动拉起 dsh**：通过 `npx --yes @deepseek-ai/dsh web --port 3080` 启动（npx 每次自动用最新版 dsh）
- **桌面窗口承载界面**：dsh 就绪后，窗口直接加载其 Web UI，无需浏览器标签页
- **首次启动向导**：检测不到 Node.js 时，可一键安装便携版 Node.js（下载到应用自己的目录，不动系统），或引导手动安装
- **系统托盘**：关闭窗口最小化到托盘，dsh 服务继续后台运行；托盘菜单可打开主窗口 / 快问 / 开机自启 / 退出
- **系统通知**：快问任务完成或失败时发送系统通知
- **⌨️ 快问弹窗（Alt+Space）**：任何界面下按 `Alt+Space` 弹出小输入框，输入任务回车即后台执行（走 dsh headless 单次问答），结果实时回流，完成弹通知——像 Raycast/Listary 一样随叫随到
- **开机自启**：托盘菜单一键开启，开机后台常驻
- **单实例**：重复双击 exe 只会聚焦已有窗口，不会起第二个 dsh

## 🖥️ 截图

> TODO: 添加截图

## 🧩 为什么"永远兼容 dsh 更新"

壳从不复制 dsh 的界面或逻辑——界面是 dsh 自己吐出来的 Web UI，壳只是把它放进一个原生窗口。
dsh 更新 → 下次启动 `npx` 自动用新版 → 壳照常工作。**dsh 本体从不进入 exe 安装包**。

## 🚀 快速开始（用户）

1. 下载最新 [Release](https://github.com/HsgtLgt/dsh-desktop/releases) 中的 exe
2. 双击运行即可
3. （可选）托盘菜单勾选"开机自启"

## 🛠️ 开发

环境要求：Windows 10/11、[Node.js](https://nodejs.org)、[Rust](https://rustup.rs)

```bash
npm install
npm run tauri dev        # 开发模式
npm run tauri build      # 打包（产物在 src-tauri/target/release/，安装器在 target/release/bundle/）
```

### 环境变量（调试用）

| 变量 | 作用 |
|---|---|
| `DSH_DESKTOP_PORT` | 覆盖 dsh 服务端口（默认 3080） |
| `DSH_DESKTOP_LOG_DIR` | 开启文件日志，输出到指定目录 |

## 🏗️ 架构

```
┌─────────────────────────────────────────────┐
│  DSH Desktop (Tauri 2, Rust)                │
│  ├─ 主窗口 ── 加载 dsh 的 Web UI            │
│  ├─ 快问弹窗 ── 独立小窗，headless 问答      │
│  ├─ 系统托盘 ── 常驻 + 菜单                 │
│  └─ 生命周期 ── 拉起/监控/清理 dsh 进程     │
└────────────────────┬────────────────────────┘
                     │ npx 拉取 + 健康轮询
        ┌────────────▼────────────┐
        │  dsh (@deepseek-ai/dsh) │  ← 永远是 npm 最新版
        │  http://127.0.0.1:3080  │
        └─────────────────────────┘
```

## 📄 许可证

[MIT](LICENSE) © 2026 蒙 寸尘 (HsgtLgt)
