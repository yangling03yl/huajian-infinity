import type { Ctx } from '@milkdown/ctx';
import type { Editor as MilkdownEditor } from '@milkdown/core';
import { editorSel, wordCount } from './stores';
import {
  fontSizeSchema,
  fontSizeKeymap,
  remarkFontSizeSpanPlugin,
  setFontSizeCommand,
} from './font-size';

export interface EditorInstance {
  /** 同步获取当前 markdown（用于切换/关闭时 flush） */
  getMarkdown: () => string;
  /** 整体替换当前内容（用于恢复快照版本） */
  setMarkdown: (md: string) => void;
  /** 在光标处插入文本 */
  insertText: (text: string) => void;
  /** 设置当前块标题级别，0 = 正文 */
  setHeading: (level: number) => void;
  /** 设置选中文字/光标格式的字号，null = 恢复默认 */
  setFontSize: (size: number | null) => void;
  destroy: () => Promise<void>;
}

const AUTO_SAVE_MS = 1200;

function countChars(text: string): number {
  return [...text].filter((c) => !/\s/u.test(c)).length;
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
    { commonmark, headingKeymap, paragraphKeymap, wrapInHeadingCommand, turnIntoTextCommand },
    { gfm },
    { nord },
    { listener, listenerCtx },
    { history },
    { Selection },
  ] = await Promise.all([
    import('@milkdown/core'),
    import('@milkdown/preset-commonmark'),
    import('@milkdown/preset-gfm'),
    import('@milkdown/theme-nord'),
    import('@milkdown/plugin-listener'),
    import('@milkdown/plugin-history'),
    import('@milkdown/prose/state'),
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
        DowngradeHeading: { shortcuts: ['Delete', 'Backspace'] },
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
      ctx.get(listenerCtx).updated((_ctx, doc) => {
        wordCount.set(countChars(doc.textContent));
      });
      ctx.get(listenerCtx).selectionUpdated((_ctx, selection) => {
        const parent = selection.$from.parent;
        const heading = parent.type.name === 'heading' ? (parent.attrs.level as number) : 0;
        let fontSize: number | null = null;
        for (const mark of selection.$from.marks()) {
          if (mark.type.name === 'fontSizeSpan') {
            fontSize = mark.attrs.size as number;
            break;
          }
        }
        editorSel.set({ heading, fontSize });
      });
    })
    .use(nordPlugin)
    .use(commonmark)
    .use(gfm)
    .use(listener)
    .use(history)
    .use(fontSizeSchema)
    .use(remarkFontSizeSpanPlugin)
    .use(setFontSizeCommand)
    .use(fontSizeKeymap)
    .create();

  // 空花笺：光标自动置于第 0 行第 0 个字符并聚焦
  if (initialMd.trim().length === 0) {
    editor.action((ctx) => {
      const view = ctx.get(editorViewCtx);
      const doc = view.state.doc;
      view.dispatch(view.state.tr.setSelection(Selection.atStart(doc)));
      view.focus();
    });
  }

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
      editor.action((ctx) => {
        const view = ctx.get(editorViewCtx);
        const doc = ctx.get(parserCtx)(md);
        view.dispatch(view.state.tr.replaceWith(0, view.state.doc.content.size, doc));
      });
      lastMd = md;
    },
    insertText: (text: string) => {
      editor.action((ctx) => {
        const view = ctx.get(editorViewCtx);
        view.dispatch(view.state.tr.insertText(text));
      });
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
        commands.call(setFontSizeCommand.key, size);
      });
    },
    destroy: async () => {
      if (timer) clearTimeout(timer);
      await editor.destroy();
    },
  };
}
