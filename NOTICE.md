# 第三方组件与声明

本仓库的构建产物会打包以下第三方组件。分发产物时请一并保留各自的许可与版权声明。

## DeepSeek Harness (DSH)

- 来源：<https://github.com/deepseek-ai/deepseek-harness>
- 许可：MIT
- 版权：DeepSeek
- 用途：应用主体（`apps/desktop`、`apps/desktop-host`）与全部 `@deepseek-ai/*` 运行时包
- 说明：本仓库不修改官方源码，只在其产物上做必要的运行时修补（见 `scripts/postbuild.mjs`）

## Electron

- 来源：<https://github.com/electron/electron>
- 许可：MIT
- 用途：桌面应用外壳（本方案使用 Electron 44.x）

## Node.js

- 来源：<https://nodejs.org/>
- 许可：MIT
- 用途：随包分发的内置运行时（默认 v24.17.0），用于运行 DSH Host 进程

## pnpm

- 来源：<https://github.com/pnpm/pnpm>
- 许可：MIT
- 用途：随包分发，供桌面端管理插件 profile

## 其他

运行时依赖树中包含若干 npm 包，各自许可见其 `package.json`。常见的有 MIT / ISC / Apache-2.0。

---

## 商标

「DeepSeek」「DeepSeek Harness」名称与鲸鱼标识归 DeepSeek 所有。
本仓库是**社区构建方案，不是官方发行版**，请勿将其产物表述为官方安装包。

## 图标来源

`icon/source-favicon.svg` 取自 `@deepseek-ai/dsh-web-frontend/dist/favicon.svg`（DSH 官方前端资源），
由 `icon/make-ico.mjs` 渲染合成多尺寸 `.ico`。
