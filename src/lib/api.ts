import { invoke } from '@tauri-apps/api/core';
import type { AppSettings, BoxInfo, NoteMeta, SnapshotMeta } from './types';

export function errMsg(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

export const api = {
  openBox: (path: string) => invoke<BoxInfo>('open_box', { path }),
  /** 新建花匣：名字由后端从文件名派生，前端不再传 name */
  createBox: (path: string) => invoke<BoxInfo>('create_box', { path }),
  /** 重命名花匣：真改磁盘 .hxl 文件名，返回更新后的 info（id 不变） */
  renameBox: (boxId: string, newName: string) =>
    invoke<BoxInfo>('rename_box', { boxId, newName }),
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
  reorderNotes: (boxId: string, order: string[]) =>
    invoke<void>('reorder_notes', { boxId, order }),
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
  /** 一次性启动提示（数据迁移成功/失败）；后端读取后即清空 */
  getStartupNotice: () => invoke<string | null>('get_startup_notice'),
  /** 某个花匣打不开：从「下次自动打开」列表里移除，避免每次启动重复报错 */
  dropBoxPath: (path: string) => invoke<void>('drop_box_path', { path }),
  /** 关窗前最后一次保存已完成，同意 Rust 侧销毁窗口 */
  flushAck: () => invoke<void>('flush_ack'),
};
