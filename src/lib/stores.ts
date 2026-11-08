import { writable, get } from 'svelte/store';
import type { BoxInfo, NoteMeta, OpenBox, Tab, WidthMode } from './types';
import { api, errMsg } from './api';
import { flushNow } from './session';
import { dropHistory, dropBoxHistories } from './note-history';

export const boxes = writable<OpenBox[]>([]);
export const tabs = writable<Tab[]>([]);
export const activeTab = writable<Tab | null>(null);
export const saveState = writable<'idle' | 'saving' | 'saved' | 'error'>('idle');
/** 正文宽度模式：left75 靠左 75%（默认）/ full 铺满 */
export const widthMode = writable<WidthMode>('left75');
/** 状态栏提示位：一次性启动提示（如旧数据迁移），写入后自动淡出 */
export const statusMsg = writable('');

/** 状态栏中段提示：进度类信息（如导出），写入后 5 秒自动清空，无内容时不显示 */
export const statusHint = writable('');
/** 中段提示停留时长（毫秒） */
const HINT_MS = 5000;
let hintTimer: ReturnType<typeof setTimeout> | null = null;

/** 显示一条中段提示；连续调用会重置 5 秒计时，不做排队 */
export function showHint(text: string): void {
  statusHint.set(text);
  if (hintTimer !== null) clearTimeout(hintTimer);
  hintTimer = setTimeout(() => {
    hintTimer = null;
    statusHint.set('');
  }, HINT_MS);
}
/** 当前打开花笺的字数；null 表示尚未载入（状态栏以「-」占位） */
export const wordCount = writable<number | null>(null);
/** 光标所在段落的样式；heading 0 = 正文，fontSize null = 默认字号（16px） */
export const editorSel = writable<{ heading: number; fontSize: number | null }>({ heading: 0, fontSize: null });

function findBox(boxId: string): OpenBox | undefined {
  return get(boxes).find((b) => b.info.id === boxId);
}

export function boxById(boxId: string): OpenBox | undefined {
  return findBox(boxId);
}

export function noteById(boxId: string, noteId: string): NoteMeta | undefined {
  return findBox(boxId)?.notes.find((n) => n.id === noteId);
}

/**
 * 平台默认文件系统是否大小写不敏感。
 *
 * Linux 上 `A.hxl` 与 `a.hxl` 是两个真实文件，**不能**把路径小写后比较，否则
 * 用户打开 `a.hxl` 会被后端去重到 `A.hxl`，编辑写进另一个文件。
 */
const CASE_INSENSITIVE_FS = /Windows|Macintosh/i.test(navigator.userAgent);

/** 两个花匣路径是否指向同一文件（分隔符统一；仅在大小写不敏感平台忽略大小写） */
export function sameBoxPath(a: string, b: string): boolean {
  const na = a.replace(/\\/g, '/');
  const nb = b.replace(/\\/g, '/');
  if (na === nb) return true;
  return CASE_INSENSITIVE_FS && na.toLowerCase() === nb.toLowerCase();
}

/** 用后端返回的最新 info 整体替换对应花匣（改名后 name 与 path 同时变化） */
export function replaceBoxInfo(info: BoxInfo): void {
  boxes.update((list) => list.map((b) => (b.info.id === info.id ? { ...b, info } : b)));
}

export async function openBox(info: BoxInfo): Promise<void> {
  boxes.update((list) =>
    list.some((b) => b.info.id === info.id)
      ? list
      : [...list, { info, notes: [], expanded: true }],
  );
  await refreshNotes(info.id);
}

export async function refreshNotes(boxId: string): Promise<void> {
  try {
    const notes = await api.listNotes(boxId);
    boxes.update((list) =>
      list.map((b) =>
        b.info.id === boxId ? { ...b, notes, info: { ...b.info, note_count: notes.length } } : b,
      ),
    );
  } catch (e) {
    alert(`刷新花笺列表失败: ${errMsg(e)}`);
  }
}

export function toggleBox(boxId: string): void {
  boxes.update((list) =>
    list.map((b) => (b.info.id === boxId ? { ...b, expanded: !b.expanded } : b)),
  );
}

/** 按新顺序更新某花匣内花笺的展示顺序（持久化由调用方通过 api.reorderNotes 完成） */
export function reorderNotesUI(boxId: string, order: string[]): void {
  boxes.update((list) =>
    list.map((b) => {
      if (b.info.id !== boxId) return b;
      const byId = new Map(b.notes.map((n) => [n.id, n]));
      const next: NoteMeta[] = [];
      for (const id of order) {
        const n = byId.get(id);
        if (n) {
          next.push(n);
          byId.delete(id);
        }
      }
      // order 未覆盖的花笺保持原有相对顺序追加在末尾
      next.push(...b.notes.filter((n) => byId.has(n.id)));
      return { ...b, notes: next };
    }),
  );
}

export function removeNoteUI(boxId: string, noteId: string): void {
  // 该花笺的会话内修改历史一并作废
  dropHistory(`${boxId}/${noteId}`);
  boxes.update((list) =>
    list.map((b) =>
      b.info.id === boxId
        ? {
            ...b,
            info: { ...b.info, note_count: Math.max(0, b.info.note_count - 1) },
            notes: b.notes.filter((n) => n.id !== noteId),
          }
        : b,
    ),
  );
  tabs.update((list) => list.filter((t) => !(t.boxId === boxId && t.noteId === noteId)));
  const act = get(activeTab);
  if (act && act.boxId === boxId && act.noteId === noteId) {
    const remaining = get(tabs);
    activeTab.set(remaining.length > 0 ? remaining[remaining.length - 1] : null);
  }
}

export async function removeBoxUI(boxId: string): Promise<void> {
  // 顺序很重要：必须先把当前编辑内容落盘，再关闭后端花匣。
  // 反过来的话，切走标签时触发的那次 flush 会写进一个已经关闭的花匣并静默失败，
  // 用户最后敲的字就丢了。
  await flushNow();
  try {
    await api.closeBox(boxId);
  } catch {
    // close_box 只会在「花匣未打开」时失败：后端已无此花匣，UI 仍要清掉，
    // 否则会卡住一个关不掉的花匣
  }
  // 花匣内所有花笺的会话内修改历史一并作废
  dropBoxHistories(boxId);
  boxes.update((list) => list.filter((b) => b.info.id !== boxId));
  tabs.update((list) => list.filter((t) => t.boxId !== boxId));
  const act = get(activeTab);
  if (act?.boxId === boxId) {
    const remaining = get(tabs);
    activeTab.set(remaining.length > 0 ? remaining[remaining.length - 1] : null);
  }
}

export function openNote(boxId: string, noteId: string): void {
  const exists = get(tabs).some((t) => t.boxId === boxId && t.noteId === noteId);
  if (!exists) {
    tabs.update((list) => [...list, { boxId, noteId }]);
  }
  const t = get(tabs).find((x) => x.boxId === boxId && x.noteId === noteId);
  if (t) activeTab.set(t);
}

export function closeTab(tab: Tab): void {
  tabs.update((list) => list.filter((t) => !(t.boxId === tab.boxId && t.noteId === tab.noteId)));
  const act = get(activeTab);
  if (act && act.boxId === tab.boxId && act.noteId === tab.noteId) {
    const remaining = get(tabs);
    activeTab.set(remaining.length > 0 ? remaining[remaining.length - 1] : null);
  }
}

export function patchNote(boxId: string, noteId: string, patch: Partial<NoteMeta>): void {
  boxes.update((list) =>
    list.map((b) =>
      b.info.id === boxId
        ? { ...b, notes: b.notes.map((n) => (n.id === noteId ? { ...n, ...patch } : n)) }
        : b,
    ),
  );
}
