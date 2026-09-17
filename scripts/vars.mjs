/**
 * 统一路径推导。
 *
 * 约定（仓库根 = 本文件所在目录的上一级）：
 *   <repo>/scripts/vars.mjs
 *   <repo>/dsh-desktop-src/                      ← bootstrap.mjs 拉到的工作树（不入库）
 *   <repo>/dsh-desktop-src/apps/desktop          ← Electron 壳（官方源码）
 *   <repo>/dsh-desktop-src/apps/desktop-host     ← 私有 Host（官方源码）
 *   <repo>/installer/                            ← 我们写的安装器
 *   <repo>/icon/                                 ← 图标
 *
 * 所有脚本都从这里取路径，不要在别处硬编码本机目录。
 */
import { existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/** 仓库根目录。 */
export const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * 定位构建工作树。
 *
 * 推荐布局是把工作树放在仓库内（<repo>/dsh-desktop-src，已被 .gitignore 忽略）；
 * 也兼容与仓库同级（<repo>/../dsh-desktop-src）以及 DSH_WORK 环境变量指定的位置。
 * 这样脚本既能在标准 clone 里跑，也能在已有工作树的机器上直接跑。
 */
function resolveWork() {
  if (process.env.DSH_WORK) {
    return resolve(process.env.DSH_WORK);
  }
  const candidates = [
    join(ROOT, 'dsh-desktop-src'),
    join(ROOT, '..', 'dsh-desktop-src'),
  ];
  // 「已就绪」的判据有两条：官方源码在位，或工具链已装好（bootstrap 会写入根 package.json）。
  // 只看 apps/desktop 是不够的 —— 清理旧源码后 apps 会被删掉，此时探测会误判到另一个候选目录，
  // 于是 bootstrap 在一处空目录重新拉源码、而 node_modules 留在原来那处，两处都对不上。
  const ready = (candidate) => existsSync(join(candidate, 'apps', 'desktop'))
    || existsSync(join(candidate, 'node_modules', 'typescript'));
  for (const candidate of candidates) {
    if (ready(candidate)) {
      return candidate;
    }
  }
  return candidates[0];
}

/** 构建工作树（由 bootstrap.mjs 准备，已加入 .gitignore）。 */
export const WORK = resolveWork();
/** Electron 壳源码目录。 */
export const APP = join(WORK, 'apps', 'desktop');
/** 私有 Host 源码目录。 */
export const HOST = join(WORK, 'apps', 'desktop-host');
/** 壳的构建与打包中间目录。 */
export const BUILD_ROOT = join(APP, '.desktop-build');
/** 目标平台目录（当前只支持 win-x64）。 */
export const TARGET = join(BUILD_ROOT, 'targets', 'win-x64');
/** electron-builder 输出目录。 */
export const ARTIFACTS = join(TARGET, 'artifacts');
/** 未打包的应用目录。 */
export const UNPACKED = join(ARTIFACTS, 'win-unpacked');
/** 安装包组装暂存目录。 */
export const STAGE = join(BUILD_ROOT, 'sfx-stage');
/** 安装器源码目录。 */
export const INSTALLER = join(ROOT, 'installer');
/** 图标目录。 */
export const ICON_DIR = join(ROOT, 'icon');

/** 工作树里的 TypeScript 编译器入口。 */
export const TSC = join(WORK, 'node_modules', 'typescript', 'bin', 'tsc');
/** 工作树里的 esbuild 可执行文件。 */
export const ESBUILD = join(WORK, 'node_modules', '@esbuild', 'win32-x64', 'esbuild.exe');
/** 当前 Node 可执行文件。 */
export const NODE = process.execPath;

/** 官方 DSH 版本，可用环境变量 DSH_VERSION 覆盖。 */
export const DSH_VERSION = process.env.DSH_VERSION ?? '0.1.6-alpha.2';
/** 对应的官方 tag。 */
export const DSH_TAG = process.env.DSH_TAG ?? ('dsh-v' + DSH_VERSION);

