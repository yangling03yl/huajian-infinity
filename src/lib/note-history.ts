import { writable } from 'svelte/store';

/**
 * 花笺修改历史（仅存于当前会话内存，不落盘、无任何列表界面，对用户不可见）。
 *
 * - 每篇花笺（按 boxId/noteId 区分）保存一条历史链：
 *   entries[0] 为本次会话打开该花笺时的基线内容，其后每项对应一次修改后的内容；
 * - 最多保留最近 HISTORY_MAX_MODS 次修改，超出时丢弃最旧的修改，基线始终保留；
 * - 退回/前进只是移动 index 指针，本身不产生新记录；
 *   退回后再输入新内容，则丢弃被退回跳过的「前进」记录（与常规撤销/重做一致）；
 * - 关闭标签页历史保留，同一会话内重新打开可继续退回/前进；
 *   若重开时内容与记录不符（可能被外部修改），以当前内容为基线重建历史。
 */

/** 最多保存的修改次数（不含打开时的基线） */
export const HISTORY_MAX_MODS = 10;

export interface NoteHistoryState {
  /** 是否可退回（存在更早的记录） */
  canBack: boolean;
  /** 是否可前进（之前退回过，可撤回退回） */
  canForward: boolean;
}

/** 工具栏退回/前进按钮的可用状态；由 EditorPane 在切换/记录/移动后调用 publishHistoryState 刷新 */
export const noteHistoryState = writable<NoteHistoryState>({ canBack: false, canForward: false });

interface NoteHistory {
  entries: string[];
  /** 当前所处的位置下标 */
  index: number;
}

const histories = new Map<string, NoteHistory>();

/** 刷新按钮状态；key 为空字符串表示当前未打开任何花笺 */
export function publishHistoryState(key: string): void {
  const h = key ? histories.get(key) : undefined;
  noteHistoryState.set({
    canBack: !!h && h.index > 0,
    canForward: !!h && h.index < h.entries.length - 1,
  });
}

/** 打开花笺载入内容后调用：首次建立基线；同一会话内重开且内容一致则延续既有历史 */
export function initHistory(key: string, md: string): void {
  const h = histories.get(key);
  if (h && h.entries[h.index] === md) return;
  histories.set(key, { entries: [md], index: 0 });
}

/**
 * 记录一次修改（在各保存点调用）。
 * 与当前位置内容相同则忽略（退回/前进引发的重写不会混入历史）；
 * 处于退回状态时输入新内容，会截断其后的「前进」记录。
 */
export function recordModification(key: string, md: string): void {
  const h = histories.get(key);
  // 尚无基线（initHistory 未建立）时不记录，避免把中途内容误当基线
  if (!h) return;
  if (h.entries[h.index] === md) return;
  h.entries = [...h.entries.slice(0, h.index + 1), md];
  // 只保留最近十次修改：基线 entries[0] 永远保留，超出时丢弃最旧的修改
  while (h.entries.length - 1 > HISTORY_MAX_MODS) h.entries.splice(1, 1);
  h.index = h.entries.length - 1;
}

/** 退回：回到上一次修改之后、当前修改之前的内容；无可退回时返回 null */
export function historyBack(key: string): string | null {
  const h = histories.get(key);
  if (!h || h.index <= 0) return null;
  h.index -= 1;
  return h.entries[h.index];
}

/** 前进：撤回退回，恢复到退回前的内容；无可前进时返回 null */
export function historyForward(key: string): string | null {
  const h = histories.get(key);
  if (!h || h.index >= h.entries.length - 1) return null;
  h.index += 1;
  return h.entries[h.index];
}

/** 删除花笺时清理其历史 */
export function dropHistory(key: string): void {
  histories.delete(key);
}

/** 关闭花匣时清理其下所有花笺的历史 */
export function dropBoxHistories(boxId: string): void {
  const prefix = `${boxId}/`;
  for (const k of [...histories.keys()]) {
    if (k.startsWith(prefix)) histories.delete(k);
  }
}
