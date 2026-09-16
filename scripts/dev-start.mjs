#!/usr/bin/env node
/**
 * 开发态启动：直接用工作树里的 Electron 跑壳，不需要安装。
 *
 * 关键点：开发态必须设置 DSH_DESKTOP_HOST_INSPECT_PORT ——
 * 壳只有在它存在时才会给 Host 子进程传 --allow-linked-profile，
 * 否则 Host 会以 "profile bundle resolved outside the Desktop runtime and profile" 拒绝启动。
 *
 * 用法：
 *   node scripts/dev-start.mjs          启动
 *   node scripts/dev-start.mjs --check  启动并校验是否加载出 DSH 界面后退出
 */
import { spawn } from 'node:child_process';
import { createWriteStream, existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { APP, ROOT, WORK } from './vars.mjs';

const ELECTRON = join(WORK, 'node_modules', 'electron', 'dist', 'electron.exe');
const DIAG = join(ROOT, 'desktop-diag.log');
const DIAGNOSTIC = join(ROOT, 'desktop-diagnostic.txt');
const USER_DATA = join(process.env.LOCALAPPDATA ?? ROOT, 'dsh-desktop-dev');
const check = process.argv.includes('--check');

if (!existsSync(join(APP, 'lib', 'main.js'))) {
  console.error('缺少构建产物，请先运行: node scripts/build-shell.mjs');
  process.exit(1);
}
if (!existsSync(ELECTRON)) {
  console.error('缺少 Electron，请先运行: node scripts/bootstrap.mjs');
  process.exit(1);
}

rmSync(DIAG, { force: true });
rmSync(DIAGNOSTIC, { force: true });
mkdirSync(USER_DATA, { recursive: true });

const env = { ...process.env };
delete env.ELECTRON_RUN_AS_NODE;
Object.assign(env, {
  DSH_HOME: join(APP, '.desktop-build', 'development', 'home'),
  DSH_DESKTOP_NODE_BINARY: process.execPath,
  DSH_DESKTOP_DSH_DIR: join(APP, '.desktop-build', 'development', 'project'),
  DSH_DESKTOP_PNPM_ENTRY: join(process.env.APPDATA ?? '', 'npm', 'node_modules', 'pnpm', 'bin', 'pnpm.mjs'),
  DSH_DESKTOP_OPEN_DEVTOOLS: '0',
  DSH_DESKTOP_DIAGNOSTIC_FILE: DIAGNOSTIC,
  DSH_DESKTOP_HOST_INSPECT_PORT: '9230',
  DSH_DESKTOP_RENDERER_DEBUG_PORT: '9222',
  DSH_DESKTOP_MAIN_INSPECT_PORT: '9229',
  ELECTRON_ENABLE_LOGGING: '1',
});

const args = ['--inspect=127.0.0.1:9229', '--remote-debugging-port=9222', '--user-data-dir=' + USER_DATA, APP];
const child = spawn(ELECTRON, args, {
  cwd: APP,
  env,
  stdio: check ? ['ignore', 'pipe', 'pipe'] : 'inherit',
  detached: !check,
});

if (!check) {
  child.unref();
  console.log('DSH 桌面端已启动 (PID ' + child.pid + ')');
  console.log('  DSH_HOME = ' + env.DSH_HOME);
  process.exit(0);
}

child.stdout.pipe(createWriteStream(join(ROOT, 'el-out.log')));
child.stderr.pipe(createWriteStream(join(ROOT, 'el-err.log')));

let verdict = 'not-verified';
for (let i = 0; i < 60; i++) {
  await new Promise((resolve) => setTimeout(resolve, 1000));
  if (child.exitCode !== null) { verdict = 'exited code=' + child.exitCode; break; }
  if (i >= 8 && i % 3 === 0) {
    try {
      const response = await fetch('http://127.0.0.1:9222/json/list', { signal: AbortSignal.timeout(4000) });
      const targets = await response.json();
      const page = targets.find((target) => target.type === 'page');
      if (page && page.url.includes('dsh-app://app/')) {
        const socket = new WebSocket(page.webSocketDebuggerUrl);
        await new Promise((resolve, reject) => {
          socket.addEventListener('open', resolve);
          socket.addEventListener('error', reject);
          setTimeout(() => reject(new Error('timeout')), 5000);
        });
        let seq = 0;
        const waiters = new Map();
        socket.addEventListener('message', (event) => {
          const message = JSON.parse(event.data);
          const waiter = waiters.get(message.id);
          if (waiter) { waiters.delete(message.id); waiter(message); }
        });
        const call = (method, params) => {
          const id = ++seq;
          return new Promise((resolve) => {
            waiters.set(id, resolve);
            socket.send(JSON.stringify({ id, method, params }));
            setTimeout(() => resolve({ error: 'timeout' }), 6000);
          });
        };
        await call('Runtime.enable', {});
        const result = await call('Runtime.evaluate', {
          expression: 'document.title + " :: " + location.href',
          returnByValue: true,
        });
        const value = result.result && result.result.result ? result.result.result.value : '';
        socket.close();
        if (String(value).includes('DeepSeek Harness')) { verdict = 'ok: ' + value; break; }
      }
    } catch { /* 后端未就绪，继续等 */ }
  }
}
console.log('校验结果: ' + verdict);
console.log('诊断日志: ' + DIAG);
child.kill();
process.exit(verdict.startsWith('ok') ? 0 : 2);
