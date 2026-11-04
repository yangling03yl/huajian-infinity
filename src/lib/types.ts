export interface NoteMeta {
  id: string;
  title: string;
  color: string;
  file: string;
  created_at: string;
  updated_at: string;
}

export interface BoxInfo {
  id: string;
  path: string;
  name: string;
  note_count: number;
}

export interface OpenBox {
  info: BoxInfo;
  notes: NoteMeta[];
  expanded: boolean;
}

export interface Tab {
  boxId: string;
  noteId: string;
}

/** 正文宽度模式：left75（靠左 75%，默认）/ full（铺满） */
export type WidthMode = 'left75' | 'full';

export interface AppSettings {
  theme: string;
  dark: boolean;
  sidebar_width: number;
  /** 正文宽度模式 */
  width_mode: WidthMode;
  /** 已打开花匣的有序路径（启动时按此顺序自动打开） */
  boxes: string[];
}

/** 快照元信息（记录在 md 文件目录段中） */
export interface SnapshotMeta {
  id: string;
  version: number;
  label: string;
  created_at: string;
}
