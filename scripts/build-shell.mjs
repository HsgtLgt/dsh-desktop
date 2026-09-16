#!/usr/bin/env node
/**
 * 编译 Electron 壳与 desktop-host，并做必要的后处理。
 *
 * 前置：node scripts/bootstrap.mjs
 * 用法：node scripts/build-shell.mjs
 *
 * 产物：
 *   dsh-desktop-src/apps/desktop-host/lib/index.js
 *   dsh-desktop-src/apps/desktop/lib/main.js
 */
import { execFileSync } from 'node:child_process';
import { rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { APP, HOST, NODE, TSC } from './vars.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));

function step(title, run) {
  console.log('\n== ' + title + ' ==');
  run();
}

step('编译 desktop-host', () => {
  rmSync(join(HOST, 'lib'), { recursive: true, force: true });
  execFileSync(NODE, [TSC, '-p', 'tsconfig.assembly.json'], { cwd: HOST, stdio: 'inherit' });
});

step('编译 Electron 壳（ESM）', () => {
  rmSync(join(APP, 'lib'), { recursive: true, force: true });
  execFileSync(NODE, [TSC, '-p', 'tsconfig.assembly.json'], { cwd: APP, stdio: 'inherit' });
});

// 官方产物直接跑不起来，见 scripts/postbuild.mjs 顶部的说明。
step('后处理（Electron 导入 / 内置 Node / 入口包装 / 主窗口可见）', () => {
  execFileSync(NODE, [join(HERE, 'postbuild.mjs')], { stdio: 'inherit' });
});

step('打包 preload 并复制 renderer 资源', () => {
  execFileSync(NODE, [join(HERE, 'bundle-preload.mjs')], { stdio: 'inherit' });
});

console.log('\n构建完成:');
console.log('  host : ' + join(HOST, 'lib', 'index.js'));
console.log('  shell: ' + join(APP, 'lib', 'main.js'));
