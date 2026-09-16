/**
 * 把两个 preload 打成自包含 CJS，并复制 renderer 资源。
 *
 * 为什么必须打包：preload.ts 会 import 运行时常量 DESKTOP_IPC，
 * 直接 tsc 会在产物里留下 require("./ipc.ts")，运行时必崩。
 */
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import { join } from 'node:path';
import { APP, ESBUILD } from './vars.mjs';

if (!fs.existsSync(ESBUILD)) {
  throw new Error('找不到 esbuild: ' + ESBUILD + '\n  → 先运行 node scripts/bootstrap.mjs');
}

for (const name of ['preload', 'preload-app']) {
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

fs.mkdirSync(join(APP, 'lib', 'renderer'), { recursive: true });
for (const file of ['startup.html', 'startup.js', 'startup.css', 'plugin-manager.html', 'plugin-manager.js', 'plugin-manager.css']) {
  fs.copyFileSync(join(APP, 'renderer', file), join(APP, 'lib', 'renderer', file));
}
console.log('preload 已打包，renderer 资源已复制');
