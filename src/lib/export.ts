import { marked } from 'marked';
import htmlToPdfmake from 'html-to-pdfmake';
import pdfMake from 'pdfmake/build/pdfmake';
import regularFontUrl from '../assets/fonts/NotoSansSC-Regular.ttf?url';
import boldFontUrl from '../assets/fonts/NotoSansSC-Bold.ttf?url';

/** 字节 → base64（分块，避免参数栈溢出） */
export function bytesToBase64(bytes: Uint8Array): string {
  let bin = '';
  const CHUNK = 0x8000;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    bin += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(bin);
}

/** UTF-8 字符串 → base64（导出 md 用） */
export function strToBase64(text: string): string {
  return bytesToBase64(new TextEncoder().encode(text));
}

/** 正文 markdown → HTML（GFM：表格、任务列表、删除线等） */
export function mdToHtml(md: string): string {
  const html = marked.parse(md, { gfm: true, breaks: false, async: false }) as string;
  // 应用内没有本地图片，导出时以替代文本代替图片标签，避免 PDF 生成报错
  return html
    .replace(/<img[^>]*alt="([^"]*)"[^>]*>/g, '[$1]')
    .replace(/<img[^>]*>/g, '');
}

let fontsPromise: Promise<Record<string, string>> | null = null;

/** 惰性加载 PDF 用中文字体（Noto Sans SC 常规+粗体），转为 base64 vfs */
function loadPdfFonts(): Promise<Record<string, string>> {
  fontsPromise ??= (async () => {
    const [regular, bold] = await Promise.all([
      fetch(regularFontUrl).then((r) => r.arrayBuffer()),
      fetch(boldFontUrl).then((r) => r.arrayBuffer()),
    ]);
    return {
      'NotoSansSC-Regular.ttf': bytesToBase64(new Uint8Array(regular)),
      'NotoSansSC-Bold.ttf': bytesToBase64(new Uint8Array(bold)),
    };
  })();
  return fontsPromise;
}

/** pdfmake 段落样式（继承 defaultStyle 的字体与行高） */
const PDF_STYLES: Record<string, Record<string, unknown>> = {
  p: { margin: [0, 0, 0, 4] },
  h1: { fontSize: 21, bold: true, margin: [0, 14, 0, 9], color: '#1b1b1b' },
  h2: { fontSize: 16, bold: true, margin: [0, 12, 0, 7], color: '#242424' },
  h3: { fontSize: 13.5, bold: true, margin: [0, 10, 0, 5], color: '#2d2d2d' },
  h4: { fontSize: 12, bold: true, margin: [0, 8, 0, 4], color: '#363636' },
  h5: { fontSize: 11, bold: true, margin: [0, 7, 0, 4], color: '#404040' },
  h6: { fontSize: 10.5, bold: true, margin: [0, 7, 0, 4], color: '#4a4a4a' },
  blockquote: { margin: [4, 4, 4, 8], color: '#555555' },
  code: { background: '#f2efe9', color: '#353535', fontSize: 9.5 },
  pre: { background: '#f2efe9', color: '#353535', fontSize: 9.5 },
  table: { fontSize: 10 },
  th: { bold: true, background: '#f2efe9' },
  a: { color: '#3a6ea5' },
};

/**
 * 正文 markdown → PDF 字节（A4，仅正文，无快照与目录）
 */
export async function buildPdf(md: string, title: string): Promise<Uint8Array> {
  const vfs = await loadPdfFonts();
  const html = mdToHtml(md);
  const content = htmlToPdfmake(`<html><head></head><body>${html}</body></html>`, {
    defaultStyles: PDF_STYLES,
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
    pageSize: 'A4' as const,
    pageMargins: [60, 56, 60, 60] as [number, number, number, number],
    info: { title, creator: '花笺infinity' },
    defaultStyle: {
      font: 'NotoSansSC' as const,
      fontSize: 10.5,
      lineHeight: 1.55,
      color: '#262626',
    },
    styles: PDF_STYLES,
    content,
    footer: (currentPage: number, pageCount: number) => ({
      text: `${currentPage} / ${pageCount}`,
      alignment: 'center',
      fontSize: 9,
      color: '#999999',
      margin: [0, 10, 0, 0],
    }),
  };

  const blob: Blob = await new Promise((resolve, reject) => {
    try {
      pdfMake.createPdf(dd).getBlob((b: Blob) => resolve(b));
    } catch (e) {
      reject(e);
    }
  });
  return new Uint8Array(await blob.arrayBuffer());
}

/** 导出文件名清洗（去除路径非法字符） */
export function safeFileName(name: string): string {
  const cleaned = name.replace(/[\\/:*?"<>|\r\n]+/g, ' ').trim();
  return cleaned.length > 0 ? cleaned : '未命名花笺';
}
