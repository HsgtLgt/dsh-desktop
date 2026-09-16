#!/usr/bin/env node
/**
 * 物化应用要携带的 dsh 运行时树（<target>/dsh/）。
 *
 * 做三件事：
 *   1. 把本机已安装的 @deepseek-ai/dsh 整棵树复制进来
 *   2. 把 desktop-host 用 esbuild 打成单个自包含 ESM 放进去
 *   3. **扁平化**：把依赖从嵌套位置提升到运行时树顶层
 *
 * 第 3 步是必须的，原因：
 *   desktop-host 从 <runtimeDir>/node_modules/@deepseek-ai/dsh-desktop-host 加载，
 *   它 import 的 @deepseek-ai/dsh-app-boot 等包，Node 会向上找 <runtimeDir>/node_modules/。
 *   而本机安装里这些包位于 dsh/node_modules/@deepseek-ai/*（嵌套），顶层根本没有，
 *   不提升就会 ERR_MODULE_NOT_FOUND。
 *
 * 用法：node scripts/materialize-runtime.mjs
 *   DSH_INSTALL_DIR  指定已安装的 @deepseek-ai/dsh 目录（默认自动探测）
 */
import { cp, mkdir, readdir, rm, stat, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { APP, DSH_VERSION, ESBUILD, HOST, TARGET, WORK } from './vars.mjs';

/** 定位本机已安装的 @deepseek-ai/dsh。 */
function resolveDshInstall() {
  if (process.env.DSH_INSTALL_DIR) return process.env.DSH_INSTALL_DIR;
  const candidates = [];
  try {
    const root = execFileSync('npm', ['root', '-g'], { shell: true, encoding: 'utf8' }).trim();
    candidates.push(join(root, '@deepseek-ai', 'dsh'));
  } catch { /* npm 不可用时走下面的候选 */ }
  candidates.push(
    join(process.env.APPDATA ?? '', 'npm', 'node_modules', '@deepseek-ai', 'dsh'),
    join(process.env.LOCALAPPDATA ?? '', 'Programs', 'DeepSeek Harness', 'resources', 'app', 'dsh', 'node_modules', '@deepseek-ai', 'dsh'),
  );
  for (const candidate of candidates) {
    if (candidate && existsSync(join(candidate, 'package.json'))) return candidate;
  }
  throw new Error('找不到本机安装的 @deepseek-ai/dsh，请用 DSH_INSTALL_DIR 指定其目录');
}

/** 递归统计文件数。 */
async function countFiles(dir) {
  let count = 0;
  const stack = [dir];
  while (stack.length) {
    const current = stack.pop();
    let entries;
    try { entries = await readdir(current, { withFileTypes: true }); } catch { continue; }
    for (const entry of entries) {
      const path = join(current, entry.name);
      const info = await stat(path).catch(() => undefined);
      if (!info) continue;
      if (info.isDirectory()) stack.push(path);
      else count++;
    }
  }
  return count;
}

const installDir = resolveDshInstall();
const runtimeRoot = join(TARGET, 'dsh');
const topModules = join(runtimeRoot, 'node_modules');
const nestedModules = join(topModules, '@deepseek-ai', 'dsh', 'node_modules');

console.log('DSH 安装目录: ' + installDir);
console.log('[1/4] 复制 dsh 运行时树 ...');
await rm(runtimeRoot, { recursive: true, force: true });
await mkdir(join(topModules, '@deepseek-ai'), { recursive: true });
await cp(installDir, join(topModules, '@deepseek-ai', 'dsh'), { recursive: true, dereference: true, force: true });
console.log('    dsh 已复制');

console.log('[2/4] 构建并放置 desktop-host ...');
if (!existsSync(ESBUILD)) throw new Error('找不到 esbuild：' + ESBUILD + '\n  → 先运行 node scripts/bootstrap.mjs');
const hostOut = join(topModules, '@deepseek-ai', 'dsh-desktop-host');
await mkdir(join(hostOut, 'lib'), { recursive: true });
await mkdir(join(hostOut, 'config'), { recursive: true });
execFileSync(ESBUILD, [
  'apps/desktop-host/src/index.ts',
  '--bundle', '--platform=node', '--format=esm', '--target=node22', '--packages=external',
  '--outfile=' + join(hostOut, 'lib', 'index.js'), '--log-level=warning',
], { cwd: WORK, stdio: 'inherit' });
await cp(join(HOST, 'config', 'desktop.cordis.patch.yml'), join(hostOut, 'config', 'desktop.cordis.patch.yml'));
await writeFile(join(hostOut, 'package.json'), JSON.stringify({
  name: '@deepseek-ai/dsh-desktop-host',
  description: 'Private Node-mode host process for the Electron desktop application',
  version: DSH_VERSION,
  private: true,
  license: 'MIT',
  type: 'module',
  main: 'lib/index.js',
}, undefined, 2) + '\n', 'utf8');
console.log('    desktop-host 已就位');

console.log('[3/4] 扁平化依赖到运行时树顶层 ...');
const exists = (path) => existsSync(path);
let hoisted = 0;
let skipped = 0;
if (exists(nestedModules)) {
  for (const entry of await readdir(nestedModules, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    if (entry.name.startsWith('@')) {
      const scopeSrc = join(nestedModules, entry.name);
      const scopeDst = join(topModules, entry.name);
      await mkdir(scopeDst, { recursive: true });
      for (const inner of await readdir(scopeSrc, { withFileTypes: true })) {
        if (!inner.isDirectory()) continue;
        const dst = join(scopeDst, inner.name);
        if (exists(dst)) { skipped++; continue; }
        await cp(join(scopeSrc, inner.name), dst, { recursive: true, force: true });
        hoisted++;
      }
      continue;
    }
    const dst = join(topModules, entry.name);
    if (exists(dst)) { skipped++; continue; }
    await cp(join(nestedModules, entry.name), dst, { recursive: true, force: true });
    hoisted++;
  }
}
console.log('    提升 ' + hoisted + ' 个包，跳过（顶层已存在）' + skipped + ' 个');

console.log('[4/4] 统计 ...');
console.log('    resources/dsh 文件数 = ' + (await countFiles(runtimeRoot)));
console.log('    运行时树: ' + runtimeRoot);
