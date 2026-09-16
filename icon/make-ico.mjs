#!/usr/bin/env node
/**
 * 由官方 favicon.svg 生成整套应用图标（多尺寸 PNG + 多尺寸 ICO）。
 *
 * 合成方式：DeepSeek 蓝 #4D6BFE 圆角方底 + 白色鲸鱼，四角留白，小尺寸下也清晰。
 * ICO 为 PNG 压缩格式（Vista+ 支持），一次写入 256/128/64/48/32/24/16 七个尺寸。
 *
 * 用法：node icon/make-ico.mjs
 *   SHARP_DIR  手动指定 sharp 包目录（默认自动查找）
 */
import { createRequire } from 'node:module';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, '..');
const SOURCE = join(HERE, 'source-favicon.svg');

/** 定位 sharp：先找构建工作树，再找本机全局安装的 dsh（它依赖 sharp）。 */
function resolveSharpDir() {
  if (process.env.SHARP_DIR) return process.env.SHARP_DIR;
  const candidates = [join(ROOT, 'dsh-desktop-src', 'node_modules', 'sharp')];
  try {
    const globalRoot = execFileSync('npm', ['root', '-g'], { shell: true, encoding: 'utf8' }).trim();
    candidates.push(join(globalRoot, '@deepseek-ai', 'dsh', 'node_modules', 'sharp'));
    candidates.push(join(globalRoot, 'sharp'));
  } catch { /* npm 不可用时忽略 */ }
  candidates.push(join(process.env.APPDATA ?? '', 'npm', 'node_modules', '@deepseek-ai', 'dsh', 'node_modules', 'sharp'));
  for (const candidate of candidates) {
    if (candidate && existsSync(join(candidate, 'package.json'))) return candidate;
  }
  throw new Error('找不到 sharp，请先运行 node scripts/bootstrap.mjs，或用 SHARP_DIR 指定');
}

const sharp = createRequire(join(resolveSharpDir(), 'package.json'))('sharp');

// 取出官方 logo 的路径数据
const svg = readFileSync(SOURCE, 'utf8');
const match = /<path[^>]*\sd="([^"]+)"/.exec(svg);
if (!match) throw new Error('无法从 ' + SOURCE + ' 提取 path 数据');
const pathData = match[1];

const BACKGROUND = '#4D6BFE';
const FOREGROUND = '#FFFFFF';
const PADDING = 13; // 100 单位坐标系里的四边留白

/** 生成一张带底色与留白的 SVG。 */
const compose = (size, background, foreground, padding) => {
  const inner = 100 - padding * 2;
  const scale = inner / 50;
  return '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="' + size + '" height="' + size + '">'
    + '<rect width="100" height="100" rx="22" ry="22" fill="' + background + '"/>'
    + '<g transform="translate(' + padding + ', ' + padding + ') scale(' + scale + ')">'
    + '<path d="' + pathData + '" fill="' + foreground + '"/></g></svg>';
};

const SIZES = [256, 128, 64, 48, 32, 24, 16];
const pngDir = join(HERE, 'icon-png');
mkdirSync(pngDir, { recursive: true });

// 高质量主图，再下采样，小尺寸边缘更干净
const master = await sharp(Buffer.from(compose(1024, BACKGROUND, FOREGROUND, PADDING))).png().toBuffer();
writeFileSync(join(HERE, 'icon-1024.png'), master);
console.log('icon-1024.png 已生成');

const entries = [];
for (const size of SIZES) {
  const buffer = await sharp(master)
    .resize(size, size, { fit: 'contain', background: { r: 0, g: 0, b: 0, alpha: 0 } })
    .png({ compressionLevel: 9 })
    .toBuffer();
  writeFileSync(join(pngDir, String(size).padStart(3, '0') + '.png'), buffer);
  entries.push({ size, data: buffer });
  console.log('  ' + size + 'x' + size + ' -> ' + buffer.length + ' 字节');
}

// 组装 ICO 容器
const header = Buffer.alloc(6);
header.writeUInt16LE(0, 0);
header.writeUInt16LE(1, 2);          // type: icon
header.writeUInt16LE(entries.length, 4);

const directory = Buffer.alloc(16 * entries.length);
let offset = 6 + directory.length;
entries.forEach((entry, index) => {
  const base = 16 * index;
  directory[base] = entry.size >= 256 ? 0 : entry.size;   // 0 表示 256
  directory[base + 1] = entry.size >= 256 ? 0 : entry.size;
  directory[base + 2] = 0;                                 // 调色板数
  directory[base + 3] = 0;                                 // 保留
  directory.writeUInt16LE(1, base + 4);                    // 色彩平面
  directory.writeUInt16LE(32, base + 6);                   // 位深
  directory.writeUInt32LE(entry.data.length, base + 8);
  directory.writeUInt32LE(offset, base + 12);
  offset += entry.data.length;
});

const ico = Buffer.concat([header, directory, ...entries.map((entry) => entry.data)]);
writeFileSync(join(HERE, 'icon.ico'), ico);
console.log('icon.ico 已生成: ' + ico.length + ' 字节，' + entries.length + ' 个尺寸');
