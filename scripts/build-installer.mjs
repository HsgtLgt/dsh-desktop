#!/usr/bin/env node
/**
 * 编译安装器并合成最终的自解压安装包 DSH-Setup.exe。
 *
 * 安装包结构：  [setup.exe（C# 宿主）] [payload.zip] [uint32 长度][15 字节标记]
 * setup.exe 运行时从自身末尾找标记，用系统 tar.exe 解压（原生长路径支持），
 * 再调用 install.ps1 完成安装。
 *
 * 前置：node scripts/make-payload.mjs（需要 payload.zip）
 * 用法：node scripts/build-installer.mjs
 */
import { createReadStream, createWriteStream, existsSync, mkdirSync, readFileSync, statSync } from 'node:fs';
import { copyFile, rm, writeFile } from 'node:fs/promises';
import { pipeline } from 'node:stream/promises';
import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { ARTIFACTS, ICON_DIR, INSTALLER, STAGE } from './vars.mjs';
import { writeVersionedFile } from './version-token.mjs';

const MARKER = 'DSHSFX1_PAYLOAD';
const OUTPUT = join(ARTIFACTS, 'DSH-Setup.exe');

/** 定位 .NET Framework 的 C# 编译器。 */
function resolveCsc() {
  const candidates = [
    join(process.env.WINDIR ?? 'C:\\Windows', 'Microsoft.NET', 'Framework64', 'v4.0.30319', 'csc.exe'),
    join(process.env.WINDIR ?? 'C:\\Windows', 'Microsoft.NET', 'Framework', 'v4.0.30319', 'csc.exe'),
  ];
  for (const candidate of candidates) if (existsSync(candidate)) return candidate;
  throw new Error('找不到 csc.exe（需要 .NET Framework 4.x）');
}

const payload = join(STAGE, 'payload.zip');
if (!existsSync(payload)) {
  throw new Error('缺少 ' + payload + '\n  → 先运行 node scripts/make-payload.mjs');
}
mkdirSync(ARTIFACTS, { recursive: true });
mkdirSync(STAGE, { recursive: true });

// 1) 编译 setup.exe
console.log('[1/3] 编译安装器 setup.exe ...');
// Setup.cs 带版本占位符，复制时按 vars.mjs 的 DSH_VERSION 替换；
// manifest.xml 与 setup.exe.config 原样复制。
await writeVersionedFile(join(INSTALLER, 'Setup.cs'), join(STAGE, 'Setup.cs'));
for (const name of ['manifest.xml', 'setup.exe.config']) {
  await copyFile(join(INSTALLER, name), join(STAGE, name));
}
await copyFile(join(ICON_DIR, 'icon.ico'), join(STAGE, 'icon.ico'));
const setupExe = join(STAGE, 'setup.exe');
await rm(setupExe, { force: true });
// /target:winexe = 窗口程序（无控制台窗口）；System.Windows.Forms 提供向导界面
execFileSync(resolveCsc(), [
  '/nologo', '/target:winexe', '/platform:anycpu', '/optimize+',
  '/out:' + setupExe,
  '/r:System.Windows.Forms.dll', '/r:System.Drawing.dll',
  '/win32manifest:manifest.xml',
  '/win32icon:icon.ico',
  'Setup.cs',
], { cwd: STAGE, stdio: 'inherit' });
await copyFile(join(STAGE, 'setup.exe.config'), setupExe + '.config');
console.log('    setup.exe = ' + (statSync(setupExe).size / 1024).toFixed(1) + ' KB');

// 2) 组装：setup.exe + payload.zip + 长度 + 文件数 + 标记
//    文件数写进 trailer，安装器用它算进度（不必在运行时扫描 5 万个文件）
console.log('[2/3] 合成自解压安装包 ...');
const payloadLength = statSync(payload).size;
const fileCountPath = join(STAGE, 'filecount.txt');
const payloadFiles = existsSync(fileCountPath) ? Number(readFileSync(fileCountPath, 'utf8').trim()) : 0;
if (!payloadFiles) {
  throw new Error('缺少 ' + fileCountPath + '\n  → 先运行 node scripts/make-payload.mjs');
}
const trailer = Buffer.alloc(8 + MARKER.length);
trailer.writeUInt32LE(payloadLength >>> 0, 0);
trailer.writeUInt32LE(payloadFiles >>> 0, 4);
Buffer.from(MARKER, 'ascii').copy(trailer, 8);
console.log('    载荷文件数 = ' + payloadFiles);

await rm(OUTPUT, { force: true });
const out = createWriteStream(OUTPUT);
await pipeline(createReadStream(setupExe), out, { end: false });
await pipeline(createReadStream(payload), out, { end: false });
await new Promise((resolve, reject) => out.write(trailer, (error) => (error ? reject(error) : resolve())));
await new Promise((resolve) => out.end(resolve));

// 3) 报告
const total = statSync(OUTPUT).size;
console.log('[3/3] 完成');
console.log('  安装包  : ' + OUTPUT);
console.log('  体积    : ' + (total / 1024 / 1024).toFixed(1) + ' MB');
console.log('  载荷    : ' + (payloadLength / 1024 / 1024).toFixed(1) + ' MB');
