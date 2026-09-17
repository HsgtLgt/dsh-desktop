/**
 * 打包 preload 并复制 renderer 资源。
 *
 * 为什么必须打包：preload 会 import ./ipc.ts、./preload-*.ts 这类源码模块，
 * 直接 tsc 会在产物里留下对 .ts 的引用，运行时必崩。
 *
 * alpha.2 的 preload 从 2 个变成 3 个（官方 tsdown.config.ts 的 CJS 入口组）：
 *   preload-app          产品主窗口
 *   preload-mandatory    强制更新模态窗
 *   preload-update-dialog 普通更新对话框
 * 主进程按 new URL('./preload-*.cjs', import.meta.url) 解析，
 * 所以产物必须与 lib/main.js 同目录、扩展名为 .cjs。
 */
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import { join } from 'node:path';
import { APP, ESBUILD } from './vars.mjs';

const PRELOADS = ['preload-app', 'preload-mandatory', 'preload-update-dialog'];

// alpha.2 的 renderer/ 只剩更新与登录等待页；startup.* 与 plugin-manager.* 已被官方删除。
// 其中 policy-login-loading.html 由壳用 loadFile('renderer/policy-login-loading.html') 按
// app 根相对路径加载，所以必须落在 <app>/renderer/ 下，不能只留在源码目录里。
const RENDERER = [
  'mandatory-update.html', 'mandatory-update.js', 'mandatory-update.css',
  'update-dialog.html', 'update-dialog.js', 'update-dialog.css',
  'policy-login-loading.html',
  'update-close.svg',
];

if (!fs.existsSync(ESBUILD)) {
  throw new Error('找不到 esbuild: ' + ESBUILD + '\n  → 先运行 node scripts/bootstrap.mjs');
}

for (const name of PRELOADS) {
  const source = join(APP, 'src', name + '.ts');
  if (!fs.existsSync(source)) throw new Error('缺少 preload 入口: ' + source);
  execFileSync(ESBUILD, [
    'src/' + name + '.ts',
    '--bundle',
    '--platform=node',
    '--format=cjs',
    '--target=node22',
    '--external:electron',
    '--outfile=lib/' + name + '.cjs',
    '--log-level=warning',
  ], { cwd: APP, stdio: 'inherit' });
}
console.log('preload 已打包: ' + PRELOADS.join(', '));

const rendererSource = join(APP, 'renderer');
for (const file of RENDERER) {
  const from = join(rendererSource, file);
  if (!fs.existsSync(from)) throw new Error('缺少 renderer 资源: ' + from);
}
// electron-builder 的 files 白名单已经包含 renderer/**/*，正常打包含得到；
// 这里复制一份到 lib/renderer 只是为了开发态（直接跑源码目录）也能找到页面。
fs.mkdirSync(join(APP, 'lib', 'renderer'), { recursive: true });
for (const file of RENDERER) {
  fs.copyFileSync(join(rendererSource, file), join(APP, 'lib', 'renderer', file));
}
console.log('renderer 资源已复制: ' + RENDERER.length + ' 个文件');
