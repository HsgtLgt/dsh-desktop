#!/usr/bin/env node
/**
 * 生成 desktop-runtime.json —— 运行时树的完整性清单与共享包记录。
 *
 * 直接复用官方源码 apps/desktop/src/runtime-tree.ts 的 writeDesktopRuntime，
 * 所以清单格式、哈希算法与官方完全一致（打包态会逐文件校验）。
 *
 * 用法：
 *   node --experimental-strip-types scripts/gen-runtime-descriptor.mjs
 * （Node 23+ 默认支持类型剥离，可省略 flag；22.x 需要显式加上）
 *
 * 前置：scripts/materialize-runtime.mjs 与 scripts/prepare-node.mjs 都已执行，
 *       因为共享包必须真实存在于运行时树顶层，清单才能寻址到。
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { APP, DSH_VERSION, TARGET } from './vars.mjs';
import { writeDesktopRuntime } from '../dsh-desktop-src/apps/desktop/src/runtime-tree.ts';
import { DESKTOP_HOST_PROTOCOL_VERSION } from '../dsh-desktop-src/apps/desktop/src/host-protocol.ts';

const runtimeRoot = join(TARGET, 'dsh');
const nodeRuntime = join(TARGET, 'runtime');

const versions = JSON.parse(readFileSync(join(nodeRuntime, 'versions.json'), 'utf8'));
const release = {
  schemaVersion: 1,
  version: DSH_VERSION,
  hostProtocolVersion: DESKTOP_HOST_PROTOCOL_VERSION,
  nodeVersion: versions.node,
  pnpmVersion: versions.pnpm,
};
console.log('release = ' + JSON.stringify(release));

// 共享包 = 官方 desktop-host 的 13 个依赖 + dsh 自身 + host 包本身。
// 打包态会用这些记录在 profile 里建链接，所以必须都在运行时树顶层。
const shared = [
  '@deepseek-ai/dsh',
  '@deepseek-ai/dsh-desktop-host',
  '@deepseek-ai/cordis',
  '@deepseek-ai/cordis-plugin-include',
  '@deepseek-ai/dsh-api-gateway',
  '@deepseek-ai/dsh-app-boot',
  '@deepseek-ai/dsh-client-connection',
  '@deepseek-ai/dsh-client-modules',
  '@deepseek-ai/dsh-client-ui-directory-picker-native',
  '@deepseek-ai/dsh-cmdline',
  '@deepseek-ai/dsh-host-directory-picker-native',
  '@deepseek-ai/dsh-host-webserver',
  '@deepseek-ai/dsh-launch-environment',
  '@deepseek-ai/dsh-web-frontend',
];
console.log('共享包数量 = ' + shared.length);

const started = Date.now();
const descriptor = writeDesktopRuntime(runtimeRoot, release, shared, { platform: 'win32', arch: 'x64' });
console.log('描述符已生成，耗时 ' + ((Date.now() - started) / 1000).toFixed(1) + 's');
console.log('  文件数        = ' + descriptor.files.length);
console.log('  platform/arch = ' + descriptor.platform + '/' + descriptor.arch);
console.log('  共享包        = ' + descriptor.sharedPackages.map((entry) => entry.name + '@' + entry.version).join(', '));
const totalBytes = descriptor.files.reduce((sum, file) => sum + file.bytes, 0);
console.log('  运行时体积    = ' + (totalBytes / 1024 / 1024).toFixed(1) + ' MB');
console.log('  描述符        = ' + join(runtimeRoot, 'desktop-runtime.json'));
