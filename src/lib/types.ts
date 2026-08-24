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
  dirty: boolean;
  /** 临时快照，用于切换时对比是否需要保存 */
  savedMd: string;
}

export interface AppSettings {
  theme: string;
  dark: boolean;
  sidebar_width: number;
  /** 已打开花匣的有序路径（启动时按此顺序自动打开） */
  boxes: string[];
}
