#!/usr/bin/env node
/**
 * 准备随包分发的内置运行时（<target>/runtime/）：Node.js + pnpm。
 *
 *   - Node 从 nodejs.org 下载官方 zip 并校验 SHASUMS256 里的 SHA256
 *   - pnpm 直接复制本机已安装的那份（版本记录进 versions.json）
 *
 * 用法：node scripts/prepare-node.mjs
 *   NODE_VERSION  覆盖 Node 版本（默认 24.17.0）
 *   PNPM_DIR      指定 pnpm 包目录（默认从全局 npm 根推断）
 */
import { createReadStream, createWriteStream, cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { pipeline } from 'node:stream/promises';
import { execFileSync, spawnSync } from 'node:child_process';
import { join } from 'node:path';
import { APP, TARGET } from './vars.mjs';

const BUILD_ROOT = join(APP, '.desktop-build');
const NODE_VERSION = process.env.NODE_VERSION ?? '24.17.0';
const RUNTIME = join(TARGET, 'runtime');
const DOWNLOADS = join(BUILD_ROOT, 'downloads');
const MIRROR = process.env.NODE_MIRROR ?? 'https://nodejs.org/download/release';

mkdirSync(RUNTIME, { recursive: true });
mkdirSync(DOWNLOADS, { recursive: true });

async function download(url, destination) {
  const response = await fetch(url, { signal: AbortSignal.timeout(600000) });
  if (!response.ok) throw new Error(url + ' -> HTTP ' + response.status);
  await pipeline(response.body, createWriteStream(destination));
}

// ── Node ──────────────────────────────────────────────────────────────
const folder = 'node-v' + NODE_VERSION + '-win-x64';
const archive = join(DOWNLOADS, folder + '.zip');
const sums = join(DOWNLOADS, 'node-v' + NODE_VERSION + '-SHASUMS256.txt');
const base = MIRROR + '/v' + NODE_VERSION;
if (!existsSync(sums)) { console.log('下载 SHASUMS256.txt'); await download(base + '/SHASUMS256.txt', sums); }
if (!existsSync(archive)) { console.log('下载 ' + folder + '.zip（约 30MB）'); await download(base + '/' + folder + '.zip', archive); }

const expected = readFileSync(sums, 'utf8')
  .split(/\r?\n/u)
  .find((line) => line.endsWith('  ' + folder + '.zip'))
  ?.split(/\s+/u)[0];
if (!expected) throw new Error('SHASUMS256.txt 中没有 ' + folder + '.zip');
const actual = createHash('sha256').update(readFileSync(archive)).digest('hex');
if (actual !== expected) throw new Error('Node 校验和不匹配：expected=' + expected + ' actual=' + actual);
console.log('Node zip SHA256 校验通过');

const extract = join(BUILD_ROOT, 'node-extract');
rmSync(extract, { recursive: true, force: true });
mkdirSync(extract, { recursive: true });
const unzip = spawnSync('powershell', [
  '-NoProfile', '-Command',
  'Expand-Archive -LiteralPath "' + archive + '" -DestinationPath "' + extract + '" -Force',
], { encoding: 'utf8' });
if (unzip.status !== 0) throw new Error('解压失败: ' + (unzip.stderr || unzip.stdout));

const nodeSource = join(extract, folder, 'node.exe');
const nodeDestinationDir = join(RUNTIME, 'node');
rmSync(nodeDestinationDir, { recursive: true, force: true });
mkdirSync(nodeDestinationDir, { recursive: true });
await pipeline(createReadStream(nodeSource), createWriteStream(join(nodeDestinationDir, 'node.exe')));
const version = spawnSync(join(nodeDestinationDir, 'node.exe'), ['--version'], { encoding: 'utf8' });
console.log('内置 Node: ' + version.stdout.trim() + (version.stdout.trim() === 'v' + NODE_VERSION ? ' ✓' : ' ✗ 版本不符'));
rmSync(extract, { recursive: true, force: true });

// ── pnpm ──────────────────────────────────────────────────────────────
function resolvePnpmDir() {
  if (process.env.PNPM_DIR) return process.env.PNPM_DIR;
  try {
    const root = execFileSync('npm', ['root', '-g'], { shell: true, encoding: 'utf8' }).trim();
    const candidate = join(root, 'pnpm');
    if (existsSync(join(candidate, 'package.json'))) return candidate;
  } catch { /* 继续尝试默认位置 */ }
  const fallback = join(process.env.APPDATA ?? '', 'npm', 'node_modules', 'pnpm');
  if (existsSync(join(fallback, 'package.json'))) return fallback;
  throw new Error('找不到 pnpm，请先安装（npm i -g pnpm）或用 PNPM_DIR 指定');
}

const pnpmDir = resolvePnpmDir();
const pnpmVersion = JSON.parse(readFileSync(join(pnpmDir, 'package.json'), 'utf8')).version;
const pnpmDestination = join(RUNTIME, 'pnpm');
rmSync(pnpmDestination, { recursive: true, force: true });
cpSync(pnpmDir, pnpmDestination, { recursive: true, dereference: false });
console.log('内置 pnpm: ' + pnpmVersion);

writeFileSync(
  join(RUNTIME, 'versions.json'),
  JSON.stringify({ schemaVersion: 1, node: NODE_VERSION, pnpm: pnpmVersion }, undefined, 2) + '\n',
  'utf8',
);
console.log('versions.json 已写入: ' + RUNTIME);
