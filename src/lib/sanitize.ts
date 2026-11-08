import { Marked, type RendererObject, type Tokens } from 'marked';

/**
 * 零依赖的 Markdown → 安全 HTML。
 *
 * 为什么不用 DOMPurify / sanitize-html：
 * 本项目是离线可用的 Tauri 桌面端，node_modules 里没有这两个包，也不允许为渲染再引入
 * 运行时依赖；而 `marked` 的 renderer 扩展足以在「生成 HTML 的同一处」完成白名单化——
 * 原始 HTML token 一律转义成纯文本（用户写的 HTML 看得见、但不执行），
 * 链接/图片做协议白名单校验，其余标签只可能来自 marked 默认渲染器（文本已转义）。
 *
 * 背景：`.hxl` 是可分享文件，打开别人给的花匣 → 快照预览 → 脚本在应用内执行，
 * 配合 Tauri 的 IPC 权限等于任意读写磁盘。这条路径必须堵死。
 *
 * 注意：这里必须用**对象字面量**而不是 `class X extends Renderer`。marked 的 `use()`
 * 是逐属性拷贝扩展里 renderer 的自有属性，而 class 的方法在原型上（不可枚举），
 * 继承写法会静默失效、完全不生效（已实测）。
 */

/** 允许的链接协议；不带协议说明符的 `#锚点` 与相对路径另行放行 */
const SAFE_PROTOCOLS = new Set(['http', 'https', 'mailto']);

/** `scheme:` 形式的前缀（相对路径不会匹配） */
const SCHEME_RE = /^([a-zA-Z][a-zA-Z0-9+.-]*):/;

/** 归一化 URL 前剔除的字符：C0 控制符与各类空白，防 `java\tscript:`、` javascript:` 之类绕过 */
const URL_STRIP_RE = /[\u0000-\u0020\u007f\u00a0\u2028\u2029]+/g;

const HTML_ESCAPES: Record<string, string> = {
  '&': '&amp;',
  '<': '&lt;',
  '>': '&gt;',
  '"': '&quot;',
  "'": '&#39;',
};

/** 转义 HTML 特殊字符（文本与属性值通用） */
function escapeHtml(text: string): string {
  return text.replace(/[&<>"']/g, (ch) => HTML_ESCAPES[ch]);
}

/** 归一化 URL；协议不在白名单（javascript:/data:/vbscript:/file: 等）时返回 null */
function sanitizeUrl(raw: string | null | undefined): string | null {
  if (typeof raw !== 'string') return null;
  const url = raw.replace(URL_STRIP_RE, '');
  if (url === '') return null;
  if (url.startsWith('#')) return url;
  const m = SCHEME_RE.exec(url);
  if (!m) return url; // 不以 `scheme:` 出现的相对路径
  return SAFE_PROTOCOLS.has(m[1].toLowerCase()) ? url : null;
}

/**
 * 段落字号的整段 span：应用自身写出的唯一一种合法原始 HTML。
 * 只放行「style 里恰好是 font-size: N px」这一种形态，其余属性/事件一律不认。
 */
const FONT_SPAN_OPEN_RE = /^<span\s+style="font-size:\s*\d{1,3}px"\s*>$/i;
const SPAN_CLOSE_RE = /^<\/span\s*>$/i;

const safeRenderer: RendererObject = {
  /** 原始 HTML（块级/行内都走这里）：转义成纯文本，看得见但不执行 */
  html(token) {
    const raw = token.text ?? '';
    // 字号 span 必须放行，否则快照预览与 PDF 会把整段的标签当文字显示出来。
    // 判定极窄：必须是非块级、必须完整匹配上述两条正则，不解析任意 style。
    if (!token.block) {
      const trimmed = raw.trim();
      if (FONT_SPAN_OPEN_RE.test(trimmed) || SPAN_CLOSE_RE.test(trimmed)) return raw;
    }
    const text = escapeHtml(raw);
    return token.block ? `<p>${text.replace(/\n/g, '<br>')}</p>` : text;
  },

  /** 链接：协议不安全时降级为纯文本（不带 href） */
  link(token) {
    const label = this.parser.parseInline(token.tokens);
    const url = sanitizeUrl(token.href);
    if (url === null) return label;
    const title = token.title ? ` title="${escapeHtml(token.title)}"` : '';
    return `<a href="${escapeHtml(url)}"${title}>${label}</a>`;
  },

  /** 图片：协议不安全时退化为 `[alt]` 文本 */
  image(token) {
    const alt = escapeHtml(token.text ?? '');
    const url = sanitizeUrl(token.href);
    if (url === null) return `[${alt}]`;
    const title = token.title ? ` title="${escapeHtml(token.title)}"` : '';
    return `<img src="${escapeHtml(url)}" alt="${alt}"${title}>`;
  },
};

/** 独立实例：不污染全局 `marked` 单例 */
const safeMarked = new Marked({
  gfm: true,
  breaks: false,
  async: false,
  renderer: safeRenderer,
});

/** Markdown → 安全 HTML（禁用原始 HTML，过滤危险链接协议） */
export function renderSafeMarkdown(md: string): string {
  return safeMarked.parse(md, { async: false }) as string;
}
