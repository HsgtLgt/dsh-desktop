/**
 * electron-builder 的 afterPack 钩子。
 *
 * 为什么需要它：运行时组件（dsh 依赖树 + 内置 Node/pnpm）走 extraResources 会被
 * electron-builder 的文件过滤丢掉 node_modules，所以这里直接复制进 resources/app。
 *
 * 另外 @deepseek-ai/dsh-home-paths 被官方声明为 workspace 依赖，
 * electron-builder 不会收集它，但壳的 paths.ts 在启动时就会 import 它，必须补齐。
 */
import { cp, mkdir, readdir, rm, stat } from 'node:fs/promises';
import { join } from 'node:path';
import { TARGET } from './vars.mjs';

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

export default async function afterPack(context) {
  const appDir = join(context.appOutDir, 'resources', 'app');
  await mkdir(appDir, { recursive: true });

  for (const name of ['dsh', 'runtime']) {
    const source = join(TARGET, name);
    const destination = join(appDir, name);
    await rm(destination, { recursive: true, force: true });
    const started = Date.now();
    await cp(source, destination, { recursive: true, force: true });
    const files = await countFiles(destination);
    console.log('  afterPack: ' + name + ' -> resources/app/' + name + '（' + files + ' 个文件，' + ((Date.now() - started) / 1000).toFixed(1) + 's）');
  }

  const hostPathsSource = join(TARGET, 'dsh', 'node_modules', '@deepseek-ai', 'dsh-home-paths');
  const hostPathsTarget = join(appDir, 'node_modules', '@deepseek-ai', 'dsh-home-paths');
  await mkdir(join(appDir, 'node_modules', '@deepseek-ai'), { recursive: true });
  await rm(hostPathsTarget, { recursive: true, force: true });
  await cp(hostPathsSource, hostPathsTarget, { recursive: true, force: true });
  console.log('  afterPack: 已补齐 @deepseek-ai/dsh-home-paths');
  console.log('  afterPack: 运行时资源就位');
}
