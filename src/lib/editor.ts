import type { Ctx } from '@milkdown/ctx';
import type { Editor as MilkdownEditor } from '@milkdown/core';
import type { Selection } from '@milkdown/prose/state';
import { editorSel, wordCount } from './stores';
import {
  createParagraphStylePlugins,
  headingInputGuard,
  setParagraphFontSizeCommand,
  type ParagraphSchemaLike,
} from './font-size';

export interface EditorInstance {
  /** 同步获取当前 markdown（用于切换/关闭时 flush） */
  getMarkdown: () => string;
  /** 整体替换当前内容（用于恢复快照版本） */
  setMarkdown: (md: string) => void;
  /** 设置当前段落标题级别，0 = 正文（同时回到默认字号） */
  setHeading: (level: number) => void;
  /** 设置当前段落（或选区覆盖的所有段落）的字号，null = 恢复默认 16px */
  setFontSize: (size: number | null) => void;
  /** 丢弃尚未触发的去抖自动保存（立即落盘前调用，防止旧内容后写覆盖新内容） */
  cancelPendingSave: () => void;
  destroy: () => Promise<void>;
}

const AUTO_SAVE_MS = 1200;

function countChars(text: string): number {
  return [...text].filter((c) => !/\s/u.test(c)).length;
}

/**
 * 把当前选区的块级样式同步到工具栏。
 *
 * 关键：`updated` 与 `selectionUpdated` 都要调用。只监听选区变化时，
 * 「用快捷键/下拉/Backspace 改了标题级别但光标位置没动」不会触发刷新，
 * 工具栏会一直显示旧级别——这正是此前「标题等级和正文机制不清晰」的直接原因。
 */
function syncSelection(selection: Selection): void {
  const parent = selection.$from.parent;
  const isHeading = parent.type.name === 'heading';
  const heading = isHeading ? (parent.attrs.level as number) : 0;
  const fontSize =
    !isHeading && parent.type.name === 'paragraph'
      ? ((parent.attrs.fontSize as number | null) ?? null)
      : null;
  editorSel.set({ heading, fontSize });
}

/**
 * 创建 Milkdown 所见即所得编辑器
 * @param container 挂载容器
 * @param initialMd 初始 markdown
 * @param onChange 内容变化回调（已去抖，用于自动保存）
 * @param onLive 内容变化即时回调（不去抖，用于快照实时差异）
 */
export async function createEditor(
  container: HTMLElement,
  initialMd: string,
  onChange: (md: string) => void,
  onLive?: (md: string) => void,
): Promise<EditorInstance> {
  const [
    { Editor, rootCtx, defaultValueCtx, serializerCtx, editorViewCtx, commandsCtx, parserCtx },
    {
      commonmark,
      headingKeymap,
      paragraphKeymap,
      paragraphSchema,
      wrapInHeadingCommand,
      turnIntoTextCommand,
    },
    { gfm },
    { nord },
    { listener, listenerCtx },
    { history },
    { Selection },
    { splitBlockAs },
  ] = await Promise.all([
    import('@milkdown/core'),
    import('@milkdown/preset-commonmark'),
    import('@milkdown/preset-gfm'),
    import('@milkdown/theme-nord'),
    import('@milkdown/plugin-listener'),
    import('@milkdown/plugin-history'),
    import('@milkdown/prose/state'),
    import('@milkdown/prose/commands'),
  ]);

  let timer: ReturnType<typeof setTimeout> | null = null;
  let lastMd = initialMd;

  // v7.22 要求插件返回 CtxRunner，nord 主题类型未跟进，包一层适配
  const nordPlugin = (ctx: Ctx): (() => void) => {
    nord(ctx);
    return () => {};
  };

  const editor: MilkdownEditor = await Editor.make()
    .config((ctx: Ctx) => {
      ctx.set(rootCtx, container);
      ctx.set(defaultValueCtx, initialMd);
      // 标题快捷键：Ctrl+1~6 = 标题一~六，Ctrl+0 = 正文（原默认 Mod-Alt 组合腾给字号）
      ctx.set(headingKeymap.key, {
        TurnIntoH1: { shortcuts: 'Mod-1' },
        TurnIntoH2: { shortcuts: 'Mod-2' },
        TurnIntoH3: { shortcuts: 'Mod-3' },
        TurnIntoH4: { shortcuts: 'Mod-4' },
        TurnIntoH5: { shortcuts: 'Mod-5' },
        TurnIntoH6: { shortcuts: 'Mod-6' },
        // 只保留 Backspace：Delete 在标题开头原本也会降级，用户想删字却把标题降了级
        DowngradeHeading: { shortcuts: 'Backspace' },
      });
      ctx.set(paragraphKeymap.key, {
        TurnIntoText: { shortcuts: 'Mod-0' },
      });
      ctx.get(listenerCtx).markdownUpdated((_ctx, md) => {
        lastMd = md;
        onLive?.(md);
        if (timer) clearTimeout(timer);
        timer = setTimeout(() => onChange(md), AUTO_SAVE_MS);
      });
      ctx.get(listenerCtx).updated((updatedCtx, doc) => {
        wordCount.set(countChars(doc.textContent));
        // 文档变化也要刷新样式：改标题级别 / 快照恢复 / 撤销都不会移动光标
        syncSelection(updatedCtx.get(editorViewCtx).state.selection);
      });
      ctx.get(listenerCtx).selectionUpdated((_ctx, selection) => {
        syncSelection(selection);
      });
    })
    // 必须先于 commonmark：抢在 heading 输入规则之前把 `# ` 当普通文本插入
    .use(headingInputGuard)
    .use(nordPlugin)
    .use(commonmark)
    .use(gfm)
    .use(
      createParagraphStylePlugins({
        paragraphSchema: paragraphSchema as unknown as ParagraphSchemaLike,
        splitBlockAs,
      }),
    )
    .use(listener)
    .use(history)
    .create();

  editor.action((ctx) => {
    const view = ctx.get(editorViewCtx);
    // 初始字数：字数监听只在后续事务上触发，这里先按初始文档计一次，
    // 保证刚切换到的花笺立刻显示自己的字数，而不是残留上一篇的数值
    wordCount.set(countChars(view.state.doc.textContent));
    syncSelection(view.state.selection);
    // 空花笺：光标自动置于第 0 行第 0 个字符并聚焦
    if (initialMd.trim().length === 0) {
      const doc = view.state.doc;
      view.dispatch(view.state.tr.setSelection(Selection.atStart(doc)));
      view.focus();
    }
  });

  return {
    getMarkdown: () => {
      let md = lastMd;
      editor.action((ctx) => {
        md = ctx.get(serializerCtx)(ctx.get(editorViewCtx).state.doc);
      });
      lastMd = md;
      return md;
    },
    setMarkdown: (md: string) => {
      // 整体替换属于程序性恢复（快照恢复/退回/前进）：此前排定的自动保存内容已过期，
      // 必须丢弃，否则稍后会把替换前的旧内容写回，覆盖刚恢复的正文
      if (timer) clearTimeout(timer);
      timer = null;
      editor.action((ctx) => {
        const view = ctx.get(editorViewCtx);
        const doc = ctx.get(parserCtx)(md);
        const tr = view.state.tr.replaceWith(0, view.state.doc.content.size, doc.content);
        // 恢复后不能带着替换前的光标格式：否则继续输入的新文字仍是旧字号/旧样式
        tr.setStoredMarks([]);
        tr.setSelection(Selection.atStart(tr.doc));
        view.dispatch(tr);
      });
      lastMd = md;
    },
    setHeading: (level: number) => {
      editor.action((ctx) => {
        const commands = ctx.get(commandsCtx);
        if (level === 0) commands.call(turnIntoTextCommand.key);
        else commands.call(wrapInHeadingCommand.key, level);
      });
    },
    setFontSize: (size: number | null) => {
      editor.action((ctx) => {
        const commands = ctx.get(commandsCtx);
        commands.call(setParagraphFontSizeCommand.key, size);
      });
    },
    cancelPendingSave: () => {
      if (timer) clearTimeout(timer);
      timer = null;
    },
    destroy: async () => {
      if (timer) clearTimeout(timer);
      await editor.destroy();
    },
  };
}
