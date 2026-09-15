import { writable, get } from 'svelte/store';
import type { BoxInfo, NoteMeta, OpenBox, Tab, WidthMode } from './types';
import { api } from './api';

export const boxes = writable<OpenBox[]>([]);
export const tabs = writable<Tab[]>([]);
export const activeTab = writable<Tab | null>(null);
export const saveState = writable<'idle' | 'saving' | 'saved' | 'error'>('idle');
/** 正文宽度模式：left75 靠左 75%（默认）/ full 铺满 */
export const widthMode = writable<WidthMode>('left75');
/** 状态栏提示位：只服务番茄钟（其他业务提示已移除，失败一律走 alert） */
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
export const wordCount = writable(0);
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
    boxes.update((list) => list.map((b) => (b.info.id === boxId ? { ...b, notes } : b)));
    const b = findBox(boxId);
    if (b) {
      boxes.update((list) =>
        list.map((x) => (x.info.id === boxId ? { ...x, info: { ...x.info, note_count: notes.length } } : x)),
      );
    }
  } catch (e) {
    alert(`刷新花笺列表失败: ${e}`);
  }
}

export function toggleBox(boxId: string): void {
  boxes.update((list) =>
    list.map((b) => (b.info.id === boxId ? { ...b, expanded: !b.expanded } : b)),
  );
}

export function upsertNote(boxId: string, note: NoteMeta): void {
  boxes.update((list) =>
    list.map((b) => {
      if (b.info.id !== boxId) return b;
      const exists = b.notes.some((n) => n.id === note.id);
      return {
        ...b,
        info: { ...b.info, note_count: exists ? b.info.note_count : b.info.note_count + 1 },
        notes: exists ? b.notes.map((n) => (n.id === note.id ? note : n)) : [...b.notes, note],
      };
    }),
  );
}

export function removeNoteUI(boxId: string, noteId: string): void {
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
  await api.closeBox(boxId);
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
    tabs.update((list) => [...list, { boxId, noteId, dirty: false, savedMd: '' }]);
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

export function setTabDirty(tab: Tab, dirty: boolean, savedMd?: string): void {
  tabs.update((list) =>
    list.map((t) => {
      if (t.boxId === tab.boxId && t.noteId === tab.noteId) {
        return savedMd !== undefined ? { ...t, dirty, savedMd } : { ...t, dirty };
      }
      return t;
    }),
  );
  const act = get(activeTab);
  if (act && act.boxId === tab.boxId && act.noteId === tab.noteId) {
    activeTab.set(
      savedMd !== undefined
        ? { ...act, dirty, savedMd }
        : { ...act, dirty },
    );
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
