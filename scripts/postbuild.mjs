/**
 * tsc 之后必须运行的后处理。每一条都对应"官方产物在本机跑不起来"的真实原因：
 *
 *   (a) Electron 的 package 只有 default 导出，
 *       `import { app } from 'electron'` 会抛
 *       "does not provide an export named 'app'"，必须改走 createRequire。
 *   (b) 打包态的运行期路径一律基于 app.getAppPath() ——
 *       官方写的是 process.resourcesPath，而本方案把 dsh / runtime 放在
 *       resources/app/ 下（见 afterpack.mjs），层级与官方不同。
 *   (e) 官方主窗口以 show:false 起步、就绪后再 show；
 *       一旦后端迟迟不就绪，用户只会看到"什么都没有"。改为默认可见，
 *       至少能看到错误页与恢复按钮。
 *   (f) 磁盘式打包环境文件：alpha.2 的 shell 用 loadLayeredEnv 读
 *       apps/desktop/.env.windows，且会主动剥离同名环境变量，
 *       所以必须保证该文件存在，否则启动即抛错。
 *   (g) 最后把入口包一层，把静态 import 失败变成可记录的日志。
 *
 * 顺序不能调换：(a)(b)(e)(f) 都要作用在原始 main.js 上，(g) 最后改名。
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

/**
 * (b) process.resourcesPath -> app.getAppPath()
 *
 * alpha.2 的 runtimeResources() 有三处：
 *   join(process.resourcesPath, 'runtime', 'bin')
 *   join(process.resourcesPath, 'runtime', 'pnpm', 'bin', 'pnpm.mjs')
 *   join(process.resourcesPath, 'runtime', 'primary-runtime')
 * 本方案把 runtime/ 放进 resources/app/，所以统一改写到 app 目录下。
 * 用「取子路径」而不是整串替换，避免动到与运行期无关的 resourcesPath 用法。
 */
let resourcesPathRewrites = 0;
for (const name of jsFiles()) {
  const path = join(lib, name);
  let code = fs.readFileSync(path, 'utf8');
  const before = code;
  code = code
    .replaceAll("join(process.resourcesPath, 'runtime'", "join(app.getAppPath(), 'runtime'")
    .replaceAll('join(process.resourcesPath, "runtime"', 'join(app.getAppPath(), "runtime"');
  if (code === before) continue;
  fs.writeFileSync(path, code, 'utf8');
  resourcesPathRewrites += (before.match(/process\.resourcesPath, ['"]runtime['"]/gu) ?? []).length;
  console.log('  (b) ' + name + ': 运行期路径改为 app 目录');
}
console.log('  (b) 共改写 ' + resourcesPathRewrites + ' 处运行期路径');

// (e) 主窗口默认可见
for (const name of jsFiles()) {
  const path = join(lib, name);
  const code = fs.readFileSync(path, 'utf8');
  const patched = code.replace(
    'function createWindow(preload, show = false',
    'function createWindow(preload, show = true /* SHOW-FIRST */',
  );
  if (patched === code) continue;
  fs.writeFileSync(path, patched, 'utf8');
  console.log('  (e) ' + name + ': 主窗口默认可见');
}

// (f) 磁盘式打包环境文件
const ENV_FILE = join(APP, '.env.windows');
const ENV_EXAMPLE = join(APP, '.env.windows.example');
if (!fs.existsSync(ENV_FILE)) {
  if (!fs.existsSync(ENV_EXAMPLE)) {
    throw new Error('缺少 ' + ENV_EXAMPLE + '，无法生成 .env.windows');
  }
  // 直接照抄 example：它已含必需的两项（APP_ID 与强更 test origin），
  // 其余是留给签名/上传的可选项，留空即可。
  fs.copyFileSync(ENV_EXAMPLE, ENV_FILE);
  console.log('  (f) .env.windows 已从 example 生成');
} else {
  console.log('  (f) .env.windows 已存在，保留');
}

// (g) 入口包装
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
  console.log('  (g) 入口包装已生成');
}

const finalMain = fs.readFileSync(join(lib, 'main.impl.js'), 'utf8');
console.log('postbuild 完成：运行期路径已指向 app 目录 = '
  + !finalMain.includes("process.resourcesPath, 'runtime'"));
