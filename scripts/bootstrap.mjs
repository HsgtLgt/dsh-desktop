#!/usr/bin/env node
/**
 * 准备构建工作树 dsh-desktop-src/。
 *
 * 做四件事：
 *   1. 从官方仓库按 tag 拉取 apps/desktop 与 apps/desktop-host 源码
 *      （逐文件走 raw.githubusercontent.com，比 git clone 稳，且只取需要的两个 app）
 *   2. 写入本仓库的适配文件（tsconfig.assembly*.json / package.json / .npmrc）
 *   3. 在工作树根安装构建工具链（TypeScript、esbuild、Electron、electron-builder）
 *   4. 在 apps/desktop 安装壳的运行时依赖（electron-updater、semver）
 *
 * 用法：
 *   node scripts/bootstrap.mjs                # 首次准备 / 增量补齐
 *   node scripts/bootstrap.mjs --refresh      # 重新拉取源码（保留 node_modules）
 *   DSH_VERSION=0.1.6-alpha.2 node scripts/bootstrap.mjs   # 指定版本
 */
import { cp, mkdir, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { dirname, join, relative } from 'node:path';
import { APP, HOST, ROOT, WORK, DSH_TAG, DSH_VERSION } from './vars.mjs';

const OWNER = 'deepseek-ai';
const REPO = 'deepseek-harness';
const WANTED = ['apps/desktop/', 'apps/desktop-host/'];
const RAW = 'https://raw.githubusercontent.com/' + OWNER + '/' + REPO + '/' + DSH_TAG + '/';
const refresh = process.argv.includes('--refresh');

/** 带重试的文本下载。 */
async function fetchText(url, tries = 6) {
  let last;
  for (let i = 0; i < tries; i++) {
    try {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), 30000);
      const response = await fetch(url, { signal: controller.signal });
      clearTimeout(timer);
      if (!response.ok) throw new Error('HTTP ' + response.status);
      return await response.text();
    } catch (error) {
      last = error;
      await new Promise((r) => setTimeout(r, 800 + i * 900));
    }
  }
  throw new Error(url + ' 下载失败: ' + (last && last.message));
}

/** 带重试的 JSON 下载。 */
async function fetchJson(url) {
  return JSON.parse(await fetchText(url));
}

/** 运行外部命令并继承输出。 */
function run(command, args, cwd) {
  console.log('\n$ ' + command + ' ' + args.join(' '));
  execFileSync(command, args, { cwd, stdio: 'inherit', shell: process.platform === 'win32' });
}

async function main() {
  console.log('DSH 桌面版构建准备');
  console.log('  版本 tag : ' + DSH_TAG);
  console.log('  工作树   : ' + WORK);

  // ── 1. 拉取官方源码 ────────────────────────────────────────────────
  const marker = join(WORK, '.source-ready');
  if (refresh || !existsSync(marker)) {
    console.log('\n[1/4] 拉取官方源码 ...');
    const tree = await fetchJson(
      'https://api.github.com/repos/' + OWNER + '/' + REPO + '/git/trees/' + DSH_TAG + '?recursive=1',
    );
    const blobs = tree.tree.filter(
      (entry) => entry.type === 'blob'
        && WANTED.some((prefix) => entry.path.startsWith(prefix))
        && !entry.path.includes('/tests/'),
    );
    console.log('  待下载 ' + blobs.length + ' 个文件');

    let done = 0;
    const failed = [];
    for (const blob of blobs) {
      const target = join(WORK, ...blob.path.split('/'));
      try {
        const text = await fetchText(RAW + blob.path);
        await mkdir(dirname(target), { recursive: true });
        await writeFile(target, text, 'utf8');
        done++;
      } catch (error) {
        failed.push(blob.path + ' :: ' + error.message);
      }
    }
    console.log('  完成 ' + done + '/' + blobs.length);
    if (failed.length) {
      console.error('  以下文件下载失败：\n    ' + failed.join('\n    '));
      process.exitCode = 1;
      return;
    }
    await writeFile(marker, DSH_TAG + '\n', 'utf8');
  } else {
    console.log('\n[1/4] 源码已就绪（如需重新拉取请加 --refresh）');
  }

  // ── 2. 写入适配文件 ────────────────────────────────────────────────
  console.log('\n[2/4] 写入适配文件 ...');
  const patchDir = join(ROOT, 'patch');
  await cp(join(patchDir, 'tsconfig.assembly.base.json'), join(WORK, 'tsconfig.assembly.base.json'));
  await cp(join(patchDir, 'package.json'), join(WORK, 'package.json'));
  await cp(join(patchDir, '.npmrc'), join(WORK, '.npmrc'));
  await cp(join(patchDir, 'desktop-tsconfig.assembly.json'), join(APP, 'tsconfig.assembly.json'));
  await cp(join(patchDir, 'desktop-host-tsconfig.assembly.json'), join(HOST, 'tsconfig.assembly.json'));
  console.log('  tsconfig / package.json / .npmrc 已就位');

  // ── 3. 工具链 ──────────────────────────────────────────────────────
  console.log('\n[3/4] 安装构建工具链（工作树根）...');
  run('pnpm', ['install', '--ignore-workspace'], WORK);

  // ── 4. 壳的运行时依赖 ──────────────────────────────────────────────
  // apps/desktop 的 package.json 里有 workspace:^ 依赖（属于官方 monorepo），
  // 单独安装会失败，这里临时摘掉再装。
  console.log('\n[4/4] 安装壳的运行时依赖（apps/desktop）...');
  const manifestPath = join(APP, 'package.json');
  const original = await readFile(manifestPath, 'utf8');
  const manifest = JSON.parse(original);
  const stripped = [];
  for (const field of ['dependencies', 'devDependencies']) {
    const block = manifest[field];
    if (!block) continue;
    for (const [name, spec] of Object.entries(block)) {
      if (typeof spec === 'string' && spec.startsWith('workspace:')) {
        delete block[name];
        stripped.push(field + '.' + name);
      }
    }
  }
  if (stripped.length) {
    console.log('  临时移除 monorepo 专属依赖: ' + stripped.join(', '));
    await writeFile(manifestPath, JSON.stringify(manifest, undefined, 2) + '\n', 'utf8');
  }
  try {
    // pnpm 11 默认把依赖的构建脚本列为「已忽略」，并因此以非零码退出。
    // electron-winstaller 的构建脚本对 Electron 壳没有用处，显式忽略它、同时关掉
    // strict-dep-builds 的失败语义，安装才算干净成功。
    run('pnpm', [
      'install', '--ignore-workspace', '--no-frozen-lockfile',
      '--config.strict-dep-builds=false',
      '--config.ignored-built-dependencies=electron-winstaller',
    ], APP);
  } finally {
    await writeFile(manifestPath, original, 'utf8');
    console.log('  apps/desktop/package.json 已还原');
  }

  // ── 自检 ───────────────────────────────────────────────────────────
  console.log('\n准备完成，自检：');
  const checks = [
    ['壳源码', join(APP, 'src', 'main.ts')],
    ['Host 源码', join(HOST, 'src', 'index.ts')],
    ['TypeScript', join(WORK, 'node_modules', 'typescript', 'bin', 'tsc')],
    ['esbuild', join(WORK, 'node_modules', '@esbuild', 'win32-x64', 'esbuild.exe')],
    ['Electron', join(WORK, 'node_modules', 'electron', 'dist', 'electron.exe')],
    ['electron-builder', join(APP, 'node_modules', 'electron-builder', 'out', 'cli', 'cli.js')],
  ];
  for (const [label, path] of checks) {
    console.log('  ' + (existsSync(path) ? '✓' : '✗') + ' ' + label);
  }
  console.log('\n下一步：node scripts/build-shell.mjs');
}

main().catch((error) => {
  console.error('\n准备失败: ' + (error && error.stack ? error.stack : error));
  process.exitCode = 1;
});
