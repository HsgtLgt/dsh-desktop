#!/usr/bin/env node
/**
 * 把壳源码实际 import 的 @deepseek-ai/* 工作区包，从 dsh 运行时树复制到壳的 node_modules。
 *
 * 为什么需要：官方 apps/desktop 用 tsconfig project reference 指向 monorepo 内的
 * packages/boot/app-boot 与 packages/util/home-paths，我们只有工作区里这两个 app，
 * monorepo 不存在。官方 package.json 把这两个包声明成 `workspace:^`，
 * bootstrap 装配时会临时摘掉它们（否则 pnpm 报 ERR_PNPM_WORKSPACE_PKG_NOT_FOUND），
 * 于是编译期找不到类型、运行期也解析不到。
 *
 * 做法：扫 src/ 的 import 得到需要的包名，从 DSH 运行时树按包名复制过来。
 * 复制的是完整的包（含 lib/types 类型声明），所以 tsc 与运行时都能解析。
 *
 * 用法：node scripts/sync-shell-deps.mjs
 *   DSH_INSTALL_DIR  运行时树来源（默认与 materialize-runtime.mjs 相同的探测规则）
 */
import { cp, mkdir, readdir, rm } from 'node:fs/promises';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { APP, HOST } from './vars.mjs';

const IMPORT = /from\s+['"](@deepseek-ai\/[a-z0-9][a-z0-9._~-]*)['"]/gu;

/**
 * 源码用「子路径导入」引用、因此扫不到的包：`import { runProfile } from '@deepseek-ai/dsh/profile-boot'`
 * 只会匹配到 `@deepseek-ai/dsh`，而那个包确实需要落到 node_modules 里
 * （tsconfig.assembly.base.json 的 paths 也指向它）。
 */
const ALWAYS = ['@deepseek-ai/dsh'];

/** 定位 dsh 依赖树根（含 @deepseek-ai/* 的那一层 node_modules）。 */
function resolveModuleRoot() {
  if (process.env.DSH_INSTALL_DIR) {
    const installDir = process.env.DSH_INSTALL_DIR;
    return existsSync(join(installDir, 'node_modules'))
      ? join(installDir, 'node_modules')
      : join(installDir, '..', '..');
  }
  const candidates = [];
  try {
    const root = execFileSync('npm', ['root', '-g'], { shell: true, encoding: 'utf8' }).trim();
    candidates.push(join(root, '@deepseek-ai', 'dsh', 'node_modules'));
    candidates.push(root);
  } catch { /* npm 不可用时走下面的候选 */ }
  candidates.push(join(process.env.APPDATA ?? '', 'npm', 'node_modules'));
  for (const candidate of candidates) {
    if (candidate && existsSync(join(candidate, '@deepseek-ai', 'dsh', 'package.json'))) return candidate;
  }
  throw new Error('找不到 dsh 依赖树，请用 DSH_INSTALL_DIR 指定已安装的 @deepseek-ai/dsh 目录');
}

/** 一个 app 目录下 src/ 里所有被 import 的 @deepseek-ai 包名。 */
function importedPackages(appDir) {
  const src = join(appDir, 'src');
  if (!existsSync(src)) throw new Error('缺少源码目录: ' + src);
  const names = new Set();
  for (const entry of readdirSync(src, { withFileTypes: true })) {
    if (!entry.isFile() || !entry.name.endsWith('.ts')) continue;
    const text = readFileSync(join(src, entry.name), 'utf8');
    for (const match of text.matchAll(IMPORT)) names.add(match[1]);
  }
  return [...names].sort();
}

const moduleRoot = resolveModuleRoot();
console.log('依赖树来源: ' + moduleRoot);

let copied = 0;
for (const appDir of [APP, HOST]) {
  const names = [...new Set([...importedPackages(appDir), ...ALWAYS])].sort();
  const label = appDir === APP ? 'apps/desktop' : 'apps/desktop-host';
  console.log('\n' + label + ' 需要 ' + names.length + ' 个 @deepseek-ai 包: ' + names.join(', '));
  const destinationRoot = join(appDir, 'node_modules');
  await mkdir(join(destinationRoot, '@deepseek-ai'), { recursive: true });
  for (const name of names) {
    const source = join(moduleRoot, ...name.split('/'));
    if (!existsSync(join(source, 'package.json'))) {
      throw new Error('依赖树里没有 ' + name + '（找的是 ' + source + '）\n'
        + '  → 该包是 monorepo 内部包，需要先安装进 dsh 依赖树');
    }
    // 这些包都是 workspace 内部包，不注册在 npm 上，只有整包复制一条路。
    const destination = join(destinationRoot, ...name.split('/'));
    await rm(destination, { recursive: true, force: true });
    await cp(source, destination, { recursive: true, dereference: true, force: true });
    console.log('  ✓ ' + name);
    copied++;
  }
}
console.log('\n共同步 ' + copied + ' 个包');
