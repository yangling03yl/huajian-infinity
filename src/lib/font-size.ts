import { commandsCtx } from '@milkdown/core';
import type { Ctx } from '@milkdown/ctx';
import type { Root } from '@milkdown/transformer';
import { $command, $markSchema, $remark, $useKeymap } from '@milkdown/utils';

export const FONT_SIZES = [12, 14, 16, 18, 20, 24] as const;
export const DEFAULT_FONT_SIZE = 16;

const OPEN_SPAN_RE = /^<span\b[^>]*\bstyle="[^"]*font-size\s*:\s*(\d+)px[^"]*"[^>]*>$/;
const CLOSE_SPAN_RE = /^<\/span\s*>$/;

export const fontSizeSchema = $markSchema('fontSizeSpan', () => ({
  priority: 60,
  attrs: {
    size: { default: DEFAULT_FONT_SIZE },
  },
  parseDOM: [
    {
      tag: 'span[style*="font-size"]',
      getAttrs: (dom) => {
        const el = dom as HTMLElement;
        const m = /^(\d+(?:\.\d+)?)px$/.exec(el.style.fontSize || '');
        return m ? { size: Math.round(Number(m[1])) } : null;
      },
    },
  ],
  toDOM: (mark) => ['span', { style: `font-size: ${mark.attrs.size}px` }],
  parseMarkdown: {
    match: (node) => node.type === 'fontSizeSpan',
    runner: (state, node, markType) => {
      state.openMark(markType, { size: node.size ?? DEFAULT_FONT_SIZE });
      state.next(node.children);
      state.closeMark(markType);
    },
  },
  toMarkdown: {
    match: (mark) => mark.type.name === 'fontSizeSpan',
    runner: (state, mark, node) => {
      state.addNode('html', undefined, `<span style="font-size: ${mark.attrs.size}px">`);
      state.addNode('text', undefined, node.text ?? '');
      state.addNode('html', undefined, '</span>');
      return true;
    },
  },
}));

const remarkFontSizeSpan = (): ((tree: Root) => Root) => (tree: Root) => {
  const walk = (children: unknown[]): boolean => {
    for (const child of children) {
      const c = child as { children?: unknown[] };
      if (c.children && walk(c.children)) return true;
    }
    for (let i = 0; i < children.length; i++) {
      const open = children[i] as { type?: string; value?: string } | undefined;
      if (!open || open.type !== 'html' || !open.value) continue;
      const m = OPEN_SPAN_RE.exec(open.value);
      if (!m) continue;
      for (let j = i + 1; j < children.length; j++) {
        const cur = children[j] as { type?: string; value?: string } | undefined;
        if (cur?.type === 'html' && cur.value && OPEN_SPAN_RE.test(cur.value)) break;
        if (cur?.type === 'html' && cur.value && CLOSE_SPAN_RE.test(cur.value)) {
          const size = Number(m[1]);
          const inner = children.slice(i + 1, j);
          children.splice(i, j - i + 1, { type: 'fontSizeSpan', size, children: inner });
          return true;
        }
      }
    }
    return false;
  };
  while (walk(tree.children)) {
    // keep merging nested spans until none left
  }
  return tree;
};

export const remarkFontSizeSpanPlugin = $remark('remarkFontSizeSpan', () => remarkFontSizeSpan);

export const setFontSizeCommand = $command('SetFontSize', (ctx) => (size: number | null | undefined) => (state, dispatch) => {
  const { selection, tr } = state;
  const type = fontSizeSchema.type(ctx);
  if (size == null) {
    if (selection.empty) {
      tr.removeStoredMark(type);
    } else {
      tr.removeMark(selection.from, selection.to, type);
    }
  } else {
    const mark = type.create({ size });
    if (selection.empty) {
      tr.removeStoredMark(type).addStoredMark(mark);
    } else {
      tr.addMark(selection.from, selection.to, mark);
    }
  }
  if (dispatch) dispatch(tr);
  return true;
});

const callSetFontSize = (ctx: Ctx, size: number | null) => {
  const commands = ctx.get(commandsCtx);
  return () => commands.call(setFontSizeCommand.key, size);
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
