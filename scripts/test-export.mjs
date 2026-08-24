// 导出管线端到端验证：markdown → HTML → pdfmake PDF
// 用法: node scripts/test-export.mjs [输出.pdf]
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const marked = require('marked');
const htmlToPdfmake = require('html-to-pdfmake');
const pdfMake = require('pdfmake/build/pdfmake.js');
const { DOMParser } = require('linkedom');

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..');
const out = process.argv[2] ?? join(root, 'dist', 'test-export.pdf');

const md = `# 花笺导出测试

这是**正文**，含 *斜体* 与 \`行内代码\`。

## 列表与引用

- 项目一
- 项目二

> 引用文字：花笺无限 0.4.0-beta

## 表格

| 名称 | 版本 |
| ---- | ---- |
| 快照 | v1 |
| 导出 | v2 |

\`\`\`
代码块测试
console.log('花笺');
\`\`\`

结束段落。`;

function bytesToBase64(bytes) {
  let bin = '';
  const CHUNK = 0x8000;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    bin += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(bin);
}

const regular = readFileSync(join(root, 'src/assets/fonts/NotoSansSC-Regular.ttf'));
const bold = readFileSync(join(root, 'src/assets/fonts/NotoSansSC-Bold.ttf'));
const vfs = {
  'NotoSansSC-Regular.ttf': bytesToBase64(new Uint8Array(regular)),
  'NotoSansSC-Bold.ttf': bytesToBase64(new Uint8Array(bold)),
};

const PDF_STYLES = {
  p: { margin: [0, 0, 0, 4] },
  h1: { fontSize: 21, bold: true, margin: [0, 14, 0, 9], color: '#1b1b1b' },
  h2: { fontSize: 16, bold: true, margin: [0, 12, 0, 7], color: '#242424' },
  h3: { fontSize: 13.5, bold: true, margin: [0, 10, 0, 5], color: '#2d2d2d' },
  blockquote: { margin: [4, 4, 4, 8], color: '#555555' },
  code: { background: '#f2efe9', color: '#353535', fontSize: 9.5 },
  pre: { background: '#f2efe9', color: '#353535', fontSize: 9.5 },
  table: { fontSize: 10 },
  th: { bold: true, background: '#f2efe9' },
  a: { color: '#3a6ea5' },
};

marked.setOptions({ gfm: true, breaks: false });
const html = marked.parse(md);
const content = htmlToPdfmake(`<html><head></head><body>${html}</body></html>`, {
  defaultStyles: PDF_STYLES,
  window: { DOMParser },
});

pdfMake.vfs = vfs;
pdfMake.fonts = {
  NotoSansSC: {
    normal: 'NotoSansSC-Regular.ttf',
    bold: 'NotoSansSC-Bold.ttf',
    italics: 'NotoSansSC-Regular.ttf',
    bolditalics: 'NotoSansSC-Bold.ttf',
  },
};

const dd = {
  pageSize: 'A4',
  pageMargins: [60, 56, 60, 60],
  info: { title: '花笺导出测试', creator: '花笺infinity' },
  defaultStyle: { font: 'NotoSansSC', fontSize: 10.5, lineHeight: 1.55, color: '#262626' },
  styles: PDF_STYLES,
  content,
  footer: (currentPage, pageCount) => ({
    text: `${currentPage} / ${pageCount}`,
    alignment: 'center',
    fontSize: 9,
    color: '#999999',
    margin: [0, 10, 0, 0],
  }),
};

const pdfDoc = pdfMake.createPdf(dd);
pdfDoc.getBuffer((buf) => {
  writeFileSync(out, buf);
  console.log('PDF written:', out, buf.length, 'bytes');
}, { autoPrint: false });
