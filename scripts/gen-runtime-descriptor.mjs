#!/usr/bin/env node
/**
 * 生成 desktop-runtime.json —— 运行时树的完整性清单与共享包记录。
 *
 * 直接复用官方源码 apps/desktop/src/runtime-tree.ts 的 writeDesktopRuntime，
 * 所以清单格式、哈希算法与官方完全一致（打包态会逐文件校验）。
 *
 * alpha.2 注意：desktop-host 的依赖闭包整个换过（不再有 api-gateway / cmdline /
 * client-modules / web-frontend / launch-environment / directory-picker-native，
 * 改成 agent / jobs / tools / skill-office）。这份 shared 列表必须与
 * apps/desktop-host/package.json 的 dependencies 保持一致：
 * 打包态会按它把包链接进 profile，缺一个就少一个插件运行时可用的目录。
 *
 * 用法：
 *   node --experimental-strip-types scripts/gen-runtime-descriptor.mjs
 * （Node 23+ 默认支持类型剥离，可省略 flag；22.x 需要显式加上）
 *
 * 前置：scripts/materialize-runtime.mjs 与 scripts/prepare-node.mjs 都已执行，
 *       因为共享包必须真实存在于运行时树顶层，清单才能寻址到。
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { DSH_VERSION, TARGET, WORK } from './vars.mjs';

// 工作树位置由 vars.mjs 探测（可能不在仓库内），所以源码模块必须按 WORK 动态载入，
// 不能用静态相对 import —— 那会写死 <repo>/dsh-desktop-src。
const { writeDesktopRuntime } = await import(
  pathToFileURL(join(WORK, 'apps', 'desktop', 'src', 'runtime-tree.ts')).href
);
const { DESKTOP_HOST_PROTOCOL_VERSION } = await import(
  pathToFileURL(join(WORK, 'apps', 'desktop', 'src', 'host-protocol.ts')).href
);

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

// 共享包 = 官方 desktop-host 的 10 个依赖（全部 workspace:^）+ dsh 自身 + host 包本身。
// 与 apps/desktop-host/package.json 逐项对应。
const shared = [
  '@deepseek-ai/dsh',
  '@deepseek-ai/dsh-desktop-host',
  '@deepseek-ai/cordis',
  '@deepseek-ai/dsh-agent',
  '@deepseek-ai/dsh-app-boot',
  '@deepseek-ai/dsh-client-connection',
  '@deepseek-ai/dsh-home-paths',
  '@deepseek-ai/dsh-host-webserver',
  '@deepseek-ai/dsh-jobs',
  '@deepseek-ai/dsh-skill-office',
  '@deepseek-ai/dsh-tools',
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
