/**
 * 版本号占位符的替换工具。
 *
 * 版本只允许有一个出处：`vars.mjs` 的 `DSH_VERSION`（可用环境变量覆盖）。
 * 安装器源码（`installer/Setup.cs`、`installer/install.ps1`）里写占位符，
 * 打载荷与编译安装器时替换成真实版本。
 *
 * 为什么不在源码里直接写版本号：alpha.1 → alpha.2 升级时，壳与运行时都换成了 alpha.2，
 * 但注册表里的 `DisplayVersion` 与向导副标题仍停在 alpha.1 —— 安装器看起来是旧版本。
 * 写死版本号意味着每次升级都有三处需要手工同步，漏一处就是静默的版本谎报。
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { DSH_VERSION } from './vars.mjs';

/** 安装器源码里的占位符。 */
export const VERSION_TOKEN = '__DSH_VERSION__';

/**
 * 把文本里的占位符换成当前版本。
 *
 * 占位符一个都找不到时抛错：这说明源码被改写成了写死版本号，
 * 此时静默通过会让安装包继续谎报版本。
 */
export function substituteVersion(text, label) {
  if (!text.includes(VERSION_TOKEN)) {
    throw new Error(label + ' 里没有 ' + VERSION_TOKEN + ' 占位符：版本号被写死了？'
      + '\n  → 安装器源码必须用占位符，版本只在 scripts/vars.mjs 里定义');
  }
  return text.split(VERSION_TOKEN).join(DSH_VERSION);
}

/**
 * 复制文件并把版本占位符替换成当前版本。
 *
 * 保留源文件的 UTF-8 BOM：`install.ps1` 少了 BOM 会被 PowerShell 5.1 按 ANSI 解析，
 * 中文注释乱码且脚本静默失败（见 docs/踩坑记录.md 第 14 条）。
 * 正因为丢 BOM 的后果是"静默失败"，这里对 `.ps1` **强制要求**源文件带 BOM：
 * 任何编辑器/脚本一不小心把它存成无 BOM，构建就会当场报错，而不是发出一个坏安装包。
 */
export function writeVersionedFile(source, destination) {
  const raw = readFileSync(source);
  const hasBom = raw.length >= 3 && raw[0] === 0xef && raw[1] === 0xbb && raw[2] === 0xbf;
  if (!hasBom && source.toLowerCase().endsWith('.ps1')) {
    throw new Error(source + ' 缺少 UTF-8 BOM。\n'
      + '  → PowerShell 5.1 会把无 BOM 的 .ps1 按 ANSI 解析，中文乱码且脚本静默失败\n'
      + '  → 修复：Set-Content -LiteralPath <文件> -Value (Get-Content <文件> -Raw) -NoNewline -Encoding utf8BOM');
  }
  const text = raw.subarray(hasBom ? 3 : 0).toString('utf8');
  const replaced = substituteVersion(text, source);
  const body = Buffer.from(replaced, 'utf8');
  writeFileSync(destination, hasBom ? Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), body]) : body);
  return { hasBom, version: DSH_VERSION };
}
