import { invoke } from '@tauri-apps/api/core';
import type { AppSettings, BoxInfo, NoteMeta, PomodoroDayStats, SnapshotMeta } from './types';

export function errMsg(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

export const api = {
  openBox: (path: string) => invoke<BoxInfo>('open_box', { path }),
  createBox: (path: string, name: string) => invoke<BoxInfo>('create_box', { path, name }),
  closeBox: (boxId: string) => invoke<void>('close_box', { boxId }),
  listNotes: (boxId: string) => invoke<NoteMeta[]>('list_notes', { boxId }),
  readNote: (boxId: string, noteId: string) => invoke<string>('read_note', { boxId, noteId }),
  writeNote: (boxId: string, noteId: string, content: string) =>
    invoke<void>('write_note', { boxId, noteId, content }),
  createNote: (boxId: string, title: string, color: string) =>
    invoke<NoteMeta>('create_note', { boxId, title, color }),
  renameNote: (boxId: string, noteId: string, title: string) =>
    invoke<void>('rename_note', { boxId, noteId, title }),
  deleteNote: (boxId: string, noteId: string) => invoke<void>('delete_note', { boxId, noteId }),
  setNoteColor: (boxId: string, noteId: string, color: string) =>
    invoke<void>('set_note_color', { boxId, noteId, color }),
  saveBox: (boxId: string) => invoke<void>('save_box', { boxId }),
  createSnapshot: (boxId: string, noteId: string, label: string | null) =>
    invoke<SnapshotMeta>('create_snapshot', { boxId, noteId, label }),
  listSnapshots: (boxId: string, noteId: string) =>
    invoke<SnapshotMeta[]>('list_snapshots', { boxId, noteId }),
  readSnapshot: (boxId: string, noteId: string, snapshotId: string) =>
    invoke<string>('read_snapshot', { boxId, noteId, snapshotId }),
  applySnapshot: (boxId: string, noteId: string, snapshotId: string) =>
    invoke<string>('apply_snapshot', { boxId, noteId, snapshotId }),
  deleteSnapshot: (boxId: string, noteId: string, snapshotId: string) =>
    invoke<void>('delete_snapshot', { boxId, noteId, snapshotId }),
  writeBytes: (path: string, dataBase64: string) =>
    invoke<void>('write_bytes', { path, dataBase64 }),
  getSettings: () => invoke<AppSettings>('get_settings'),
  setAppearance: (theme: string, dark: boolean, sidebarWidth: number, widthMode: string) =>
    invoke<void>('set_appearance', { theme, dark, sidebarWidth, widthMode }),
  reorderBoxes: (order: string[]) => invoke<void>('reorder_boxes', { order }),
  getStartupHx: () => invoke<string | null>('get_startup_hx'),
  revealInFolder: (path: string) =>
    invoke<void>('reveal_in_folder', { path }).catch(() => {}),
  setPomodoroSettings: (mode: string, focusMinutes: number, breakMinutes: number, loops: number) =>
    invoke<void>('set_pomodoro_settings', { mode, focusMinutes, breakMinutes, loops }),
  recordPomodoro: (seconds: number, mode: string) =>
    invoke<void>('record_pomodoro', { seconds, mode }),
  getPomodoroStats: (year: number, month: number) =>
    invoke<PomodoroDayStats[]>('get_pomodoro_stats', { year, month }),
};
