/**
 * tsc 之后必须运行的后处理。四件事都是"官方产物在本机跑不起来"的真实原因：
 *
 *   (a) Electron 的 package 只有 default 导出，
 *       `import { app } from 'electron'` 会抛
 *       "does not provide an export named 'app'"，必须改走 createRequire。
 *   (c) 打包态下 Host 必须用内置 Node 运行；用 Electron 自身当 Runtime 会被
 *       node-addon-require-builtin 以"不支持的 Electron 指纹"拒绝启动。
 *   (d) runtime 与 pnpm 一律基于 app.getAppPath() 定位；
 *       process.resourcesPath 的层级在打包态并不可靠。
 *   (e) 官方主窗口以 show:false 起步、就绪后再 show；
 *       一旦后端迟迟不就绪，用户只会看到"什么都没有"。改为默认可见，
 *       至少能看到启动页/错误页与恢复按钮。
 *   (b) 最后把入口包一层，把静态 import 失败变成可记录的日志。
 *
 * 顺序不能调换：(a)(c)(d)(e) 都要作用在原始 main.js 上，(b) 最后改名。
 */
import fs from 'node:fs';
import { join } from 'node:path';
import { APP } from './vars.mjs';

const lib = join(APP, 'lib');
const traceEnabled = process.env.DSH_DESKTOP_TRACE_FILE !== undefined;
const DIAG = process.env.DSH_DESKTOP_TRACE_FILE ?? join(APP, '.desktop-build', 'postbuild.log');
const log = (message) => {
  if (traceEnabled) {
    try { fs.appendFileSync(DIAG, message + '\n'); } catch { /* 日志失败不影响构建 */ }
  }
};

const jsFiles = () => fs.readdirSync(lib).filter((name) => name.endsWith('.js'));

// (a) electron 命名导入 -> createRequire
for (const name of jsFiles()) {
  const path = join(lib, name);
  let code = fs.readFileSync(path, 'utf8');
  const pattern = /^import\s*\{([^}]*)\}\s*from\s*['"]electron['"];?\s*$/m;
  const found = pattern.exec(code);
  if (!found) continue;
  const names = found[1].split(',').map((item) => item.trim()).filter((item) => item && !item.startsWith('type '));
  code = code.replace(pattern, [
    "import { createRequire as __cr } from 'node:module';",
    'const __req = __cr(import.meta.url);',
    'const { ' + names.join(', ') + " } = __req('electron');",
  ].join('\n'));
  fs.writeFileSync(path, code, 'utf8');
  console.log('  (a) ' + name + ': electron 导入已改写');
}

// (c) 打包态 Host 用内置 Node
for (const name of jsFiles()) {
  const path = join(lib, name);
  let code = fs.readFileSync(path, 'utf8');
  if (!code.includes(': process.execPath;')) continue;
  code = code.replace(
    ': process.execPath;',
    ": join(app.getAppPath(), 'runtime', 'node', process.platform === 'win32' ? 'node.exe' : 'node');",
  );
  fs.writeFileSync(path, code, 'utf8');
  console.log('  (c) ' + name + ': 打包态 Host 改用内置 Node');
}

// (d) pnpm 路径同样基于 app 目录
for (const name of jsFiles()) {
  const path = join(lib, name);
  let code = fs.readFileSync(path, 'utf8');
  if (!code.includes("'runtime', 'pnpm', 'bin', 'pnpm.mjs'")) continue;
  const patched = code.replaceAll(
    "join(process.resourcesPath, 'runtime', 'pnpm', 'bin', 'pnpm.mjs')",
    "join(app.getAppPath(), 'runtime', 'pnpm', 'bin', 'pnpm.mjs')",
  );
  if (patched === code) continue;
  fs.writeFileSync(path, patched, 'utf8');
  console.log('  (d) ' + name + ': pnpm 路径改为 app 目录');
}

// (e) 主窗口默认可见
for (const name of jsFiles()) {
  const path = join(lib, name);
  const code = fs.readFileSync(path, 'utf8');
  const patched = code.replace(
    'function createWindow(preload, show = false)',
    'function createWindow(preload, show = true) /* SHOW-FIRST */',
  );
  if (patched === code) continue;
  fs.writeFileSync(path, patched, 'utf8');
  console.log('  (e) ' + name + ': 主窗口默认可见');
}

// (b) 入口包装
const entry = join(lib, 'main.js');
const impl = join(lib, 'main.impl.js');
if (fs.existsSync(entry) && !fs.existsSync(impl)) {
  fs.renameSync(entry, impl);
  fs.writeFileSync(entry, [
    "import { appendFileSync } from 'node:fs';",
    'const DIAG = ' + JSON.stringify(DIAG) + ';',
    'const on = ' + String(traceEnabled) + ';',
    "const log = (message) => { if (on) { try { appendFileSync(DIAG, message + '\\n'); } catch {} } };",
    "process.on('uncaughtException', (error) => log('[uncaught] ' + (error && error.stack || String(error))));",
    "process.on('unhandledRejection', (error) => log('[unhandled] ' + (error && error.stack || String(error))));",
    'try {',
    "  await import('./main.impl.js');",
    '} catch (error) {',
    "  log('[IMPORT FAILED] ' + (error && error.stack || String(error)));",
    '  process.exitCode = 1;',
    '}',
    '',
  ].join('\n'), 'utf8');
  console.log('  (b) 入口包装已生成');
}

const finalMain = fs.readFileSync(join(lib, 'main.impl.js'), 'utf8');
console.log('postbuild 完成：打包态使用内置 Node = ' + !finalMain.includes(': process.execPath;'));
