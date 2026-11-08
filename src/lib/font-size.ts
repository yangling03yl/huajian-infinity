/**
 * 段落样式（字号）模块。
 *
 * 文件名沿用历史命名 `font-size.ts`，但实现已从「行内 mark」整体改为「段落属性」：
 *
 * - 每个段落（`paragraph`）带一个 `fontSize` attr，`null` 表示默认字号（16px）；
 * - 标题（`heading`）不带该 attr，字号由级别固定；
 * - Enter 新建段落时由 `splitBlockAs` 强制新块为默认段落，天然回到默认样式；
 * - Shift+Enter 的 `hardbreak` 是段落内的子节点，不改变段落 attr，因此样式保留。
 *
 * 为什么必须改成段落属性：
 * 旧实现用 `$markSchema('fontSizeSpan')`，并在 `toMarkdown.runner` 里手动
 * `addNode('text', node.text)` 后 `return true`。Milkdown 的 `SerializerState` 语义是
 * 「某个 mark 的 runner 返回真值后，同一 inline 节点的其余 mark 与节点自身 runner 都不再执行」，
 * 而 `hardbreak` 完全依赖节点自身 runner 输出 `break` —— 于是带字号的硬换行会被整段吞掉。
 * 改成段落 attr 后，字号与 hardbreak 彻底解耦，这类丢字从模型上消失。
 *
 * Markdown 表示（与旧文件格式保持一致，旧花匣无需迁移即可读）：
 * `<span style="font-size: 20px">整段文字</span>` 独占一个段落。
 */

import { commandsCtx, schemaCtx } from '@milkdown/core';
import type { Ctx, MilkdownPlugin } from '@milkdown/ctx';
import type { Node as ProseNode, ResolvedPos } from '@milkdown/prose/model';
import { Plugin, type Command, type EditorState, type Transaction } from '@milkdown/prose/state';
import type { ParserState, Root, SerializerState } from '@milkdown/transformer';
import { $command, $prose, $remark, $useKeymap } from '@milkdown/utils';

/** 工具栏可选字号 */
export const FONT_SIZES = [12, 14, 16, 18, 20, 24] as const;
/** 正文默认字号；与 app.css 的 --editor-font-size 保持一致 */
export const DEFAULT_FONT_SIZE = 16;
export const FONT_SIZE_MIN = 8;
export const FONT_SIZE_MAX = 96;

/** 把任意数值收进允许区间；非有限数返回默认值 */
export function clampFontSize(size: number): number {
  if (!Number.isFinite(size)) return DEFAULT_FONT_SIZE;
  return Math.min(FONT_SIZE_MAX, Math.max(FONT_SIZE_MIN, Math.round(size)));
}

/** `<span ... style="...font-size: 20px..." ...>`（行首到行尾完整匹配） */
const FONT_SPAN_OPEN_RE =
  /^<span\b[^>]*\bstyle="[^"]*font-size\s*:\s*(\d+(?:\.\d+)?)px[^"]*"[^>]*>$/i;
/** 任意 `<span ...>` 开标签（用于栈配对，避免误删用户自己写的 span） */
const ANY_SPAN_OPEN_RE = /^<span\b[^>]*>$/i;
const SPAN_CLOSE_RE = /^<\/span\s*>$/i;

/* ------------------------------------------------------------------ *
 * 一、段落 schema 扩展：新增 fontSize attr
 * ------------------------------------------------------------------ */

type SchemaFactory = (ctx: Ctx) => Record<string, unknown>;

/** `$nodeSchema(...).extendSchema` 的最小结构，避免静态依赖 preset-commonmark */
export interface ParagraphSchemaLike {
  extendSchema: (handler: (prev: SchemaFactory) => SchemaFactory) => MilkdownPlugin[];
}

interface ParagraphSchemaShape {
  attrs?: Record<string, unknown>;
  parseDOM?: unknown[];
  toDOM?: (node: ProseNode) => unknown;
}

/**
 * 段落内容序列化。
 *
 * 与 Milkdown 默认实现保持一致：段落**末尾**的硬换行不写进 Markdown（Markdown 里
 * 段落尾部的换行没有语义，写了反而会多出一个空行）。其余子节点全部交给各自的
 * runner 处理，绝不在这一层吞掉任何节点。
 */
function serializeParagraphContent(state: SerializerState, node: ProseNode): void {
  const last = node.lastChild;
  if (!(node.childCount >= 1 && last && last.type.name === 'hardbreak')) {
    state.next(node.content);
    return;
  }
  // cut 按内容偏移裁剪：最后一个 hardbreak 的 nodeSize 是 1，size - 1 正好切掉它
  state.next(node.content.cut(0, node.content.size - 1));
}

/** 为 paragraph 增加 `fontSize` 段落属性（必须在 preset-commonmark 之后 use） */
export function createParagraphFontSizeExtension(
  paragraphSchema: ParagraphSchemaLike,
): MilkdownPlugin[] {
  return paragraphSchema.extendSchema((prev) => (ctx) => {
    const base = prev(ctx) as ParagraphSchemaShape;

    const parseDOM = (base.parseDOM ?? []).map((rule) => {
      if ((rule as { tag?: string }).tag !== 'p') return rule;
      return {
        tag: 'p',
        getAttrs: (dom: HTMLElement) => {
          const raw = dom.style?.fontSize ?? '';
          const match = /^(\d+(?:\.\d+)?)px$/.exec(raw.trim());
          return match ? { fontSize: clampFontSize(Number(match[1])) } : {};
        },
      };
    });
    if (parseDOM.length === 0) parseDOM.push({ tag: 'p' });

    return {
      ...base,
      attrs: {
        ...(base.attrs ?? {}),
        fontSize: {
          default: null,
          validate: (value: unknown) => value == null || typeof value === 'number',
        },
      },
      parseDOM,
      toDOM: (node: ProseNode) => {
        const dom = base.toDOM?.(node);
        if (!Array.isArray(dom)) return dom;
        const size = node.attrs.fontSize as number | null;
        if (size == null) return dom;

        const [tag, rawAttrs, ...rest] = dom as [string, Record<string, unknown>, ...unknown[]];
        const attrs = { ...(rawAttrs ?? {}) };
        const prior = typeof attrs.style === 'string' && attrs.style.length > 0 ? `${attrs.style}; ` : '';
        attrs.style = `${prior}font-size: ${size}px`;
        return [tag, attrs, ...rest];
      },
      parseMarkdown: {
        match: (node: { type?: string }) => node.type === 'paragraph',
        runner: (state: ParserState, node: unknown, type: unknown) => {
          const mdNode = node as {
            children?: unknown[];
            value?: string;
            fontSize?: number | null;
          };
          const fontSize = mdNode.fontSize ?? null;
          state.openNode(type as never, fontSize == null ? undefined : { fontSize });
          if (mdNode.children) state.next(mdNode.children as never);
          else state.addText(mdNode.value || '');
          state.closeNode();
        },
      },
      toMarkdown: {
        match: (node: ProseNode) => node.type.name === 'paragraph',
        runner: (state: SerializerState, node: ProseNode) => {
          const size = node.attrs.fontSize as number | null;
          state.openNode('paragraph');
          if (size != null) {
            state.addNode('html', undefined, `<span style="font-size: ${size}px">`);
          }
          serializeParagraphContent(state, node);
          if (size != null) state.addNode('html', undefined, '</span>');
          state.closeNode();
        },
      },
    };
  });
}

/* ------------------------------------------------------------------ *
 * 二、旧数据迁移：行内 <span style="font-size"> → 段落字号
 * ------------------------------------------------------------------ */

interface MdNode {
  type: string;
  value?: string;
  children?: MdNode[];
  fontSize?: number | null;
}

interface SpanFrame {
  size: number | null;
  chars: number;
  open: MdNode;
}

/** 节点内可见字符数（不含空白），用于判断某个字号覆盖了多少正文 */
function visibleLength(node: MdNode): number {
  if (typeof node.value === 'string') {
    return [...node.value].filter((ch) => !/\s/u.test(ch)).length;
  }
  if (!node.children) return 0;
  return node.children.reduce((sum, child) => sum + visibleLength(child), 0);
}

/**
 * 把段落里的行内字号提升为整段字号。
 *
 * 规则（已与产品确认）：
 * - 统计每个字号覆盖的字符数，取**覆盖最多**的那个作为整段字号；
 * - 并列时取文档中**先出现**的那个；
 * - 所有与字号 span 配对的 `<span>` / `</span>` 一并移除，段落内不再保留混合字号；
 * - 未闭合的 span 不动（可能是用户自己手写的半截 HTML）。
 */
function liftParagraphFontSize(paragraph: MdNode): void {
  const children = paragraph.children;
  if (!children || children.length === 0) return;

  const stack: SpanFrame[] = [];
  const removable = new Set<MdNode>();
  const coverage = new Map<number, number>();
  const firstSeen = new Map<number, number>();
  let order = 0;

  for (const child of children) {
    if (child.type === 'html' && typeof child.value === 'string') {
      const fontOpen = FONT_SPAN_OPEN_RE.exec(child.value);
      if (fontOpen) {
        const size = clampFontSize(Number(fontOpen[1]));
        stack.push({ size, chars: 0, open: child });
        if (!firstSeen.has(size)) firstSeen.set(size, order++);
        continue;
      }
      if (ANY_SPAN_OPEN_RE.test(child.value)) {
        stack.push({ size: null, chars: 0, open: child });
        continue;
      }
      if (SPAN_CLOSE_RE.test(child.value)) {
        const frame = stack.pop();
        if (frame && frame.size != null) {
          coverage.set(frame.size, (coverage.get(frame.size) ?? 0) + frame.chars);
          removable.add(frame.open);
          removable.add(child);
        }
        continue;
      }
      continue;
    }

    const length = visibleLength(child);
    if (length === 0) continue;
    for (const frame of stack) frame.chars += length;
  }

  if (coverage.size === 0) return;

  let bestSize: number | null = null;
  let bestChars = -1;
  let bestOrder = Number.POSITIVE_INFINITY;
  for (const [size, chars] of coverage) {
    const seen = firstSeen.get(size) ?? Number.POSITIVE_INFINITY;
    if (chars > bestChars || (chars === bestChars && seen < bestOrder)) {
      bestSize = size;
      bestChars = chars;
      bestOrder = seen;
    }
  }
  if (bestSize == null) return;

  paragraph.fontSize = bestSize;
  paragraph.children = children.filter((child) => !removable.has(child));
}

/** 标题字号由级别固定：只剥离行内字号 span，不写入 attr */
function stripFontSizeSpans(node: MdNode): void {
  const children = node.children;
  if (!children || children.length === 0) return;

  const stack: { hasSize: boolean; open: MdNode }[] = [];
  const removable = new Set<MdNode>();

  for (const child of children) {
    if (child.type !== 'html' || typeof child.value !== 'string') continue;
    if (FONT_SPAN_OPEN_RE.test(child.value)) {
      stack.push({ hasSize: true, open: child });
      continue;
    }
    if (ANY_SPAN_OPEN_RE.test(child.value)) {
      stack.push({ hasSize: false, open: child });
      continue;
    }
    if (SPAN_CLOSE_RE.test(child.value)) {
      const frame = stack.pop();
      if (frame?.hasSize) {
        removable.add(frame.open);
        removable.add(child);
      }
    }
  }

  if (removable.size > 0) {
    node.children = children.filter((child) => !removable.has(child));
  }
}

/** 深度优先：先处理子节点，再处理当前节点 */
function migrateTree(node: MdNode): void {
  if (!node.children) return;
  for (const child of node.children) migrateTree(child);
  if (node.type === 'paragraph') liftParagraphFontSize(node);
  else if (node.type === 'heading') stripFontSizeSpans(node);
}

/**
 * 迁移 remark 插件。
 *
 * `@milkdown/utils` 的 `$remark` 只把插件注册进 `remarkPluginsCtx`，该切片仅被
 * **parser** 消费，不参与 stringify。因此这里可以安全地做「只读迁移」：
 * 只改 mdast，不改磁盘；用户不编辑就不会触发任何写盘。
 */
const remarkLiftParagraphFontSize = (): ((tree: Root) => Root) => (tree: Root) => {
  migrateTree(tree as unknown as MdNode);
  return tree;
};

/**
 * 对一棵 mdast 树就地做段落样式迁移。
 *
 * 生产路径由 `remarkLiftParagraphFontSize` 调用；单独导出是为了能在无 DOM 环境下
 * 直接对纯数据做回归测试（迁移规则一旦出错会静默改坏用户的字号，必须有测试兜住）。
 */
export function migrateParagraphStyles(tree: unknown): void {
  migrateTree(tree as MdNode);
}

export const remarkLiftParagraphFontSizePlugin = $remark(
  'remarkLiftParagraphFontSize',
  () => remarkLiftParagraphFontSize,
);

/* ------------------------------------------------------------------ *
 * 三、命令：设置整段字号
 * ------------------------------------------------------------------ */

interface ParagraphTarget {
  pos: number;
  attrs: Record<string, unknown>;
}

/**
 * 设置光标所在段落（或选区覆盖的所有段落）的字号。
 *
 * - 整段生效，不支持半句话字号；
 * - 光标在标题内时不生效（标题字号由级别固定），返回 false；
 * - `size = null` 表示恢复默认字号。
 */
export const setParagraphFontSizeCommand = $command(
  'SetParagraphFontSize',
  () =>
    (size: number | null | undefined) =>
    (state: EditorState, dispatch?: (tr: Transaction) => void) => {
      const next = size == null ? null : clampFontSize(size);
      const { from, to } = state.selection;
      const targets: ParagraphTarget[] = [];

      const consider = (node: ProseNode, pos: number): boolean => {
        if (node.type.name !== 'paragraph') return false;
        if (node.attrs.fontSize !== next) {
          targets.push({ pos, attrs: { ...node.attrs, fontSize: next } });
        }
        return true;
      };

      if (from === to) {
        const $from = state.doc.resolve(from);
        let hit = false;
        for (let depth = $from.depth; depth > 0; depth -= 1) {
          if (consider($from.node(depth), $from.before(depth))) {
            hit = true;
            break;
          }
        }
        if (!hit) return false;
      } else {
        state.doc.nodesBetween(from, to, (node, pos) => {
          if (consider(node, pos)) return false;
          return true;
        });
        if (targets.length === 0) {
          const $from = state.doc.resolve(from);
          if ($from.parent.type.name !== 'paragraph') return false;
        }
      }

      if (targets.length === 0) return true;
      if (dispatch) {
        const tr = state.tr;
        for (const target of targets) {
          tr.setNodeMarkup(target.pos, undefined, target.attrs);
        }
        dispatch(tr.scrollIntoView());
      }
      return true;
    },
);

const callSetFontSize = (ctx: Ctx, size: number | null) => {
  const commands = ctx.get(commandsCtx);
  return () => commands.call(setParagraphFontSizeCommand.key, size);
};

export const fontSizeKeymap = $useKeymap('fontSizeKeymap', {
  FontSize12: { shortcuts: 'Mod-Alt-1', command: (ctx) => callSetFontSize(ctx, 12) },
  FontSize14: { shortcuts: 'Mod-Alt-2', command: (ctx) => callSetFontSize(ctx, 14) },
  FontSize16: { shortcuts: 'Mod-Alt-3', command: (ctx) => callSetFontSize(ctx, 16) },
  FontSize18: { shortcuts: 'Mod-Alt-4', command: (ctx) => callSetFontSize(ctx, 18) },
  FontSize20: { shortcuts: 'Mod-Alt-5', command: (ctx) => callSetFontSize(ctx, 20) },
  FontSize24: { shortcuts: 'Mod-Alt-6', command: (ctx) => callSetFontSize(ctx, 24) },
  ClearFontSize: { shortcuts: 'Mod-Alt-0', command: (ctx) => callSetFontSize(ctx, null) },
});

/* ------------------------------------------------------------------ *
 * 四、Enter：新段落一律回到默认样式
 * ------------------------------------------------------------------ */

export type SplitBlockAsFn = (
  splitNode?: (node: ProseNode, atEnd: boolean, $from: ResolvedPos) => ProseNode | null,
) => Command;

/**
 * Enter 接管：拆分后强制新块为「默认段落」。
 *
 * - Enter（硬换行 / 段落切换）→ 新段落 `fontSize = null`、类型为 paragraph；
 * - 标题内按 Enter → 新块是正文（不会一直卡在标题里）；
 * - Shift+Enter 走 baseKeymap 的 hardbreak，段落 attr 不变，样式保留；
 * - 以下情况返回 false，把按键交还 baseKeymap：代码块、表格、空的列表项/引用块。
 */
export function createEnterKeymap(splitBlockAs: SplitBlockAsFn): MilkdownPlugin[] {
  const enterToDefault = $command(
    'EnterToDefaultParagraph',
    (ctx) =>
      () =>
      (state: EditorState, dispatch?: (tr: Transaction) => void) => {
        const { selection } = state;
        const $from = selection.$from;
        const parent = $from.parent;
        // 非文本块（文档整体被选中、或在表格/代码块里）交还默认处理；
        // 有选区的普通段落可以直接拆：splitBlockAs 内部会先删除选区再拆分。
        if (parent.type.name !== 'paragraph' && parent.type.name !== 'heading') return false;

        // 代码块换行、表格内换行交给各自插件
        for (let depth = 1; depth <= $from.depth; depth += 1) {
          const name = $from.node(depth).type.name;
          if (name === 'code_block' || name.startsWith('table')) return false;
        }

        // 空的列表项 / 引用块中的唯一空段落：交给 liftEmptyBlock 让用户能退出容器
        if (parent.content.size === 0 && $from.depth > 1) {
          const container = $from.node($from.depth - 1);
          if (container.childCount === 1) return false;
        }

        const paragraphType = ctx.get(schemaCtx).nodes.paragraph;
        if (!paragraphType) return false;

        return splitBlockAs(() => paragraphType.create())(state, dispatch);
      },
  );

  const enterKeymap = $useKeymap('enterToDefaultKeymap', {
    EnterToDefault: {
      shortcuts: 'Enter',
      // baseKeymap 的 Enter 默认 priority 50；这里必须更高才能先拿到按键
      priority: 100,
      command: (ctx) => {
        const commands = ctx.get(commandsCtx);
        return () => commands.call(enterToDefault.key);
      },
    },
  });

  // $command 返回单个 $Command 对象；$useKeymap 返回 [slice, plugin] 元组
  return [enterToDefault, ...enterKeymap];
}

/* ------------------------------------------------------------------ *
 * 五、禁用 `# ` 自动变标题
 * ------------------------------------------------------------------ */

/**
 * 拦截「行首 1~6 个 `#` + 空格」的输入规则。
 *
 * 必须在 `commonmark` **之前** use：ProseMirror 的 `handleTextInput`
 * 按插件顺序取第一个返回 true 的，抢在 headingInputRule 之前把空格作为普通文本插入。
 */
export const headingInputGuard = $prose(
  () =>
    new Plugin({
      props: {
        handleTextInput(view, from, to, text) {
          if (text !== ' ' || from !== to) return false;
          const $from = view.state.doc.resolve(from);
          const parent = $from.parent;
          if (parent.type.name !== 'paragraph') return false;
          const before = parent.textBetween(0, $from.parentOffset, '\n');
          if (!/^#{1,6}$/.test(before)) return false;
          view.dispatch(view.state.tr.insertText(' ', from, to));
          return true;
        },
      },
    }),
);

/* ------------------------------------------------------------------ *
 * 六、打包成一组插件（必须在 preset-commonmark 之后 use）
 * ------------------------------------------------------------------ */

export function createParagraphStylePlugins(options: {
  paragraphSchema: ParagraphSchemaLike;
  splitBlockAs: SplitBlockAsFn;
}): MilkdownPlugin[] {
  // $remark / $useKeymap 是 [slice, plugin] 元组；$command 是单个 $Command 对象
  return [
    ...createParagraphFontSizeExtension(options.paragraphSchema),
    ...remarkLiftParagraphFontSizePlugin,
    setParagraphFontSizeCommand,
    ...fontSizeKeymap,
    ...createEnterKeymap(options.splitBlockAs),
  ];
}
