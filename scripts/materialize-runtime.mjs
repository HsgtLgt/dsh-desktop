#!/usr/bin/env node
/**
 * 物化应用要携带的 dsh 运行时树（<target>/dsh/）。
 *
 * 官方 alpha.2 的运行时布局是「一个自包含的 node_modules + 一份完整性描述符」，
 * 等价于官方 apps/desktop/scripts/prepare-dsh.ts 的产物，但走本方案的简化路线：
 *
 *   1. node_modules/@deepseek-ai/dsh      ← 复制本机已安装的 @deepseek-ai/dsh
 *   2. node_modules/@deepseek-ai/dsh-*    ← 其余依赖本来就是扁平的，直接照搬
 *   3. node_modules/@deepseek-ai/dsh-desktop-host ← 本仓库用 esbuild 打出来的私有 Host
 *   4. desktop-runtime.json               ← 由 gen-runtime-descriptor.mjs 生成
 *
 * 与 alpha.1 的关键区别（照着旧脚本改会踩）：
 *   - 不再有 desktop-host/config/desktop.cordis.patch.yml（官方 alpha.2 已删除该覆层）
 *   - `@deepseek-ai/dsh` 是元包，依赖扁平装在 npm 的顶层 node_modules 里，
 *     包内不再有嵌套 node_modules，所以旧的"扁平化提升"在这条路径下是空操作；
 *     但为了兼容「从别处拷来的嵌套安装」，提升逻辑保留。
 *   - desktop-host 额外依赖 @deepseek-ai/dsh-skill-office，它不在 dsh 的依赖闭包里，
 *     必须由调用方先行安装（见 REQUIRED_EXTRA_PACKAGES 自检）。
 *
 * 用法：node scripts/materialize-runtime.mjs
 *   DSH_INSTALL_DIR  指定已安装的 @deepseek-ai/dsh 目录（默认自动探测）
 */
import { cp, mkdir, readdir, rm, stat, writeFile } from 'node:fs/promises';
import { existsSync, readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { APP, DSH_VERSION, ESBUILD, HOST, TARGET, WORK } from './vars.mjs';

/**
 * desktop-host 直接 import、但不属于 @deepseek-ai/dsh 依赖闭包的包。
 * 缺了它们 Host 在启动期就会 ERR_MODULE_NOT_FOUND。
 */
const REQUIRED_EXTRA_PACKAGES = ['@deepseek-ai/dsh-skill-office'];

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

/** 读取一个包的 manifest 版本，读不到返回 undefined。 */
function packageVersion(dir) {
  const manifest = join(dir, 'package.json');
  if (!existsSync(manifest)) return undefined;
  try {
    return JSON.parse(readFileSync(manifest, 'utf8')).version;
  } catch { return undefined; }
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
const installedVersion = packageVersion(installDir);
console.log('DSH 安装目录: ' + installDir);
console.log('  dsh 版本  : ' + installedVersion + '（目标 ' + DSH_VERSION + '）');
if (installedVersion !== DSH_VERSION) {
  throw new Error('本机安装的 @deepseek-ai/dsh 版本是 ' + installedVersion + '，与目标 ' + DSH_VERSION
    + ' 不一致。壳与运行时必须同版本，请先升级本机 dsh 或修正 DSH_VERSION。');
}

// 依赖既可能扁平在安装根的上层（npm 元包布局），也可能嵌套在 dsh 包内（旧布局）。
const flattenSource = existsSync(join(installDir, 'node_modules'))
  ? join(installDir, 'node_modules')
  : join(installDir, '..', '..');

const runtimeRoot = join(TARGET, 'dsh');
const topModules = join(runtimeRoot, 'node_modules');
const scopeDir = join(topModules, '@deepseek-ai');

console.log('\n[1/4] 复制 dsh 运行时树 ...');
await rm(runtimeRoot, { recursive: true, force: true });
await mkdir(scopeDir, { recursive: true });
await cp(installDir, join(scopeDir, 'dsh'), { recursive: true, dereference: true, force: true });
console.log('    dsh 已复制');

console.log('\n[2/4] 提升依赖到运行时树顶层 ...');
let hoisted = 0;
let skipped = 0;
if (existsSync(flattenSource) && flattenSource !== join(installDir, 'node_modules')) {
  // 旧布局：依赖嵌在 dsh 包内，需要提升。
  for (const entry of await readdir(flattenSource, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    if (entry.name.startsWith('@')) {
      const scopeSrc = join(flattenSource, entry.name);
      const scopeDst = join(topModules, entry.name);
      await mkdir(scopeDst, { recursive: true });
      for (const inner of await readdir(scopeSrc, { withFileTypes: true })) {
        if (!inner.isDirectory()) continue;
        const dst = join(scopeDst, inner.name);
        if (existsSync(dst)) { skipped++; continue; }
        await cp(join(scopeSrc, inner.name), dst, { recursive: true, force: true });
        hoisted++;
      }
      continue;
    }
    const dst = join(topModules, entry.name);
    if (existsSync(dst)) { skipped++; continue; }
    await cp(join(flattenSource, entry.name), dst, { recursive: true, force: true });
    hoisted++;
  }
  console.log('    提升 ' + hoisted + ' 个包，跳过（顶层已存在）' + skipped + ' 个');
} else {
  // npm 元包布局：依赖与 dsh 同级，逐个复制到运行时树顶层。
  for (const entry of await readdir(flattenSource, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    if (entry.name.startsWith('@')) {
      const scopeSrc = join(flattenSource, entry.name);
      const scopeDst = join(topModules, entry.name);
      await mkdir(scopeDst, { recursive: true });
      for (const inner of await readdir(scopeSrc, { withFileTypes: true })) {
        if (!inner.isDirectory()) continue;
        const dst = join(scopeDst, inner.name);
        if (existsSync(dst)) { skipped++; continue; }
        await cp(join(scopeSrc, inner.name), dst, { recursive: true, force: true });
        hoisted++;
      }
      continue;
    }
    const dst = join(topModules, entry.name);
    if (existsSync(dst)) { skipped++; continue; }
    await cp(join(flattenSource, entry.name), dst, { recursive: true, force: true });
    hoisted++;
  }
  console.log('    复制 ' + hoisted + ' 个包，跳过（顶层已存在）' + skipped + ' 个');
}

console.log('\n[3/4] 构建并放置 desktop-host ...');
if (!existsSync(ESBUILD)) throw new Error('找不到 esbuild：' + ESBUILD + '\n  → 先运行 node scripts/bootstrap.mjs');
const hostOut = join(scopeDir, 'dsh-desktop-host');
await mkdir(join(hostOut, 'lib'), { recursive: true });
execFileSync(ESBUILD, [
  'apps/desktop-host/src/index.ts',
  '--bundle', '--platform=node', '--format=esm', '--target=node22', '--packages=external',
  '--outfile=' + join(hostOut, 'lib', 'index.js'), '--log-level=warning',
], { cwd: WORK, stdio: 'inherit' });
await writeFile(join(hostOut, 'package.json'), JSON.stringify({
  name: '@deepseek-ai/dsh-desktop-host',
  description: 'Private Node-mode host process for the Electron desktop application',
  version: DSH_VERSION,
  private: true,
  license: 'MIT',
  type: 'module',
  main: 'lib/index.js',
  files: ['lib/index.js'],
}, undefined, 2) + '\n', 'utf8');
console.log('    desktop-host 已就位（无 config/ 覆层，alpha.2 已不需要）');

console.log('\n[4/4] 依赖自检 ...');
const missing = [];
for (const name of REQUIRED_EXTRA_PACKAGES) {
  if (!existsSync(join(topModules, ...name.split('/'), 'package.json'))) missing.push(name);
}
for (const name of ['@deepseek-ai/dsh-app-boot', '@deepseek-ai/dsh-home-paths']) {
  if (!existsSync(join(topModules, ...name.split('/'), 'package.json'))) missing.push(name);
}
if (missing.length) {
  throw new Error('运行时树缺少 desktop-host 必需的包：' + missing.join(', ')
    + '\n  → 请把它们安装进 DSH_INSTALL_DIR 所在的依赖树（见 npm-prefix 的附加安装步骤）');
}
console.log('    必需包齐备: ' + REQUIRED_EXTRA_PACKAGES.join(', ') + ', dsh-app-boot, dsh-home-paths');
console.log('    resources/dsh 文件数 = ' + (await countFiles(runtimeRoot)));
console.log('    运行时树: ' + runtimeRoot);
