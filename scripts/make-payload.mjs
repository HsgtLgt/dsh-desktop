#!/usr/bin/env node
/**
 * 生成安装包载荷 payload.zip。
 *
 * 内容 = 未打包应用 + install.ps1 + uninstall.cmd。
 * 先把 installer/ 的脚本放进 stage，再把 artifacts/win-unpacked 镜像同步过去 ——
 * 不重新同步就会把上一次的旧内容打进安装包（踩过这个坑）。
 *
 * 用法：node scripts/make-payload.mjs
 */
import { spawn } from 'node:child_process';
import { createWriteStream, existsSync, statSync, rmSync, writeFileSync } from 'node:fs';
import { mkdir, copyFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { INSTALLER, ROOT, STAGE, UNPACKED } from './vars.mjs';
import { writeVersionedFile } from './version-token.mjs';

/** 定位 7-Zip。 */
function resolveSevenZip() {
  const candidates = [
    process.env.SEVEN_ZIP,
    'C:\\Program Files\\7-Zip\\7z.exe',
    'C:\\Program Files (x86)\\7-Zip\\7z.exe',
  ].filter(Boolean);
  for (const candidate of candidates) if (existsSync(candidate)) return candidate;
  throw new Error('找不到 7z.exe，请安装 7-Zip 或用 SEVEN_ZIP 指定路径');
}

/** 递归统计文件数（不含目录）。 */
async function countFiles(dir) {
  let count = 0;
  const stack = [dir];
  while (stack.length) {
    const current = stack.pop();
    let entries;
    try { entries = await readdir(current, { withFileTypes: true }); } catch { continue; }
    for (const entry of entries) {
      const path = join(current, entry.name);
      if (entry.isDirectory()) stack.push(path);
      else count++;
    }
  }
  return count;
}

function run(command, args, cwd) {
  return new Promise((resolve) => {
    const child = spawn(command, args, { cwd, stdio: ['ignore', 'pipe', 'pipe'] });
    let text = '';
    child.stdout.on('data', (chunk) => { text += chunk.toString(); });
    child.stderr.on('data', (chunk) => { text += chunk.toString(); });
    child.on('exit', (code) => resolve({ code, text }));
  });
}

if (!existsSync(UNPACKED)) {
  throw new Error('缺少未打包应用：' + UNPACKED + '\n  → 先运行 node scripts/package-app.mjs');
}

// 1) 安装脚本进场
// stage 目录可能已被上一次构建清理掉（.desktop-build 被删或首次构建），
// 先建出来，否则 copyFile 会以 ENOENT 报「源文件不存在」这种误导性错误。
await mkdir(STAGE, { recursive: true });
// install.ps1 带版本占位符，复制时按 vars.mjs 的 DSH_VERSION 替换（并保留 UTF-8 BOM）
const versioned = writeVersionedFile(join(INSTALLER, 'install.ps1'), join(STAGE, 'install.ps1'));
await copyFile(join(INSTALLER, 'uninstall.cmd'), join(STAGE, 'uninstall.cmd'));
console.log('install.ps1 / uninstall.cmd 已放入 stage（版本 ' + versioned.version
  + '，BOM=' + versioned.hasBom + '）');

// 2) 同步未打包应用
const sync = await run('robocopy.exe', [
  UNPACKED, join(STAGE, 'win-unpacked'),
  '/MIR', '/NFL', '/NDL', '/NJH', '/NJS', '/NP', '/R:1', '/W:1',
], ROOT);
console.log('stage 同步: robocopy 退出码=' + sync.code);

// 3) 关键依赖自检
const marker = join(STAGE, 'win-unpacked', 'resources', 'app', 'node_modules', '@deepseek-ai', 'dsh-home-paths', 'package.json');
if (!existsSync(marker)) {
  throw new Error('同步结果缺少 @deepseek-ai/dsh-home-paths，终止打包（检查 afterpack 是否生效）');
}
console.log('依赖自检通过：@deepseek-ai/dsh-home-paths');

// 4) 统计文件数（供安装器显示进度，避免运行时扫描数万个文件）
const zip = join(STAGE, 'payload.zip');
rmSync(zip, { force: true });
const sevenZip = resolveSevenZip();
const unpacked = join(STAGE, 'win-unpacked');
const fileCount = await countFiles(unpacked);
writeFileSync(join(STAGE, 'filecount.txt'), String(fileCount), 'utf8');
console.log('载荷文件数 = ' + fileCount);

// 5) 压缩
// -mx=7：实测（win-x64 / alpha.2，载荷约 1.14 GB）
//   mx=1 → 417.5 MB，mx=7 → 392.6 MB，只多花约 1 分钟；
//   再往上收益很小（7z 对 zip 仍然用 Deflate），不值得再加时间。
const archive = await run(sevenZip, [
  'a', '-tzip', '-mx=7', '-mcu=on', 'payload.zip', 'win-unpacked', 'install.ps1', 'uninstall.cmd',
], STAGE);
console.log('7z 退出码=' + archive.code);
if (archive.code !== 0) {
  console.error(archive.text.slice(-2000));
  throw new Error('7z 压缩失败');
}
// 7z 被中断（或磁盘写满）时可能留下"看起来正常"的半截归档，
// 只有它自己打印的 Everything is Ok 能证明这次写入完整。
if (!archive.text.includes('Everything is Ok')) {
  console.error(archive.text.slice(-2000));
  throw new Error('7z 没有报告 Everything is Ok —— 归档可能不完整，勿用于打包');
}
console.log('payload.zip = ' + (statSync(zip).size / 1024 / 1024).toFixed(1) + ' MB');
