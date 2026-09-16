#!/usr/bin/env node
/**
 * 用 electron-builder 把壳打成未打包应用目录（--dir）。
 *
 * 为什么不走它的 NSIS 安装器：NSIS 工具链要从境外地址下载（国内基本下不动），
 * 所以安装包改由 installer/ 里的自解压方案生成，见 build-installer.mjs。
 *
 * builder.yml 里的路径先在这里替换成绝对路径再喂给 electron-builder，
 * 这样仓库里的模板保持可读，也不含任何本机路径。
 *
 * 用法：node scripts/package-app.mjs
 */
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { APP, BUILD_ROOT, ICON_DIR, NODE, ROOT, UNPACKED, WORK } from './vars.mjs';

/** YAML 标量：一律加双引号，避免空格与中文路径被误解析。 */
const yamlPath = (value) => '"' + value.replace(/\\/g, '/') + '"';

const template = readFileSync(join(ROOT, 'installer', 'builder.yml'), 'utf8');
const config = template
  .replaceAll('__ELECTRON_DIST__', yamlPath(join(WORK, 'node_modules', 'electron', 'dist')))
  .replaceAll('__AFTERPACK__', yamlPath(join(ROOT, 'scripts', 'afterpack.mjs')))
  .replaceAll('__ICON__', yamlPath(join(ICON_DIR, 'icon.ico')));

mkdirSync(BUILD_ROOT, { recursive: true });
const configPath = join(BUILD_ROOT, 'builder.generated.yml');
writeFileSync(configPath, config, 'utf8');
console.log('已生成打包配置: ' + configPath);

const cli = join(APP, 'node_modules', 'electron-builder', 'out', 'cli', 'cli.js');
execFileSync(NODE, [cli, '--config', configPath, '--dir'], { cwd: APP, stdio: 'inherit' });

console.log('\n未打包应用: ' + UNPACKED);
