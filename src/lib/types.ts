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

/** 正文宽度模式：left75 靠左 75%（默认）/ full 铺满 */
export type WidthMode = 'left75' | 'full';

export interface AppSettings {
  theme: string;
  dark: boolean;
  sidebar_width: number;
  /** 正文宽度模式 */
  width_mode: WidthMode;
  /** 已打开花匣的有序路径（启动时按此顺序自动打开） */
  boxes: string[];
  /** 番茄钟设置 */
  pomodoro: PomodoroSettings;
}

/** 番茄钟计时模式：标准（倒计时）/ 正向（正计时，手动结束） */
export type PomodoroMode = 'standard' | 'forward';

/** 番茄钟设置（对应后端 PomodoroSettings） */
export interface PomodoroSettings {
  mode: PomodoroMode;
  /** 单个番茄时长（分钟） */
  focus_minutes: number;
  /** 循环内相邻番茄之间的休息时长（分钟） */
  break_minutes: number;
  /** 一次会话的番茄个数 */
  loops: number;
}

/** 单次番茄记录（对应后端 PomodoroSession） */
export interface PomodoroSession {
  /** 完成时刻（本地时间 HH:MM） */
  at: string;
  seconds: number;
  mode: PomodoroMode;
}

/** 单日番茄统计（对应后端 PomodoroDayStats） */
export interface PomodoroDayStats {
  /** 本地日期 YYYY-MM-DD */
  date: string;
  standard_seconds: number;
  standard_count: number;
  forward_seconds: number;
  forward_count: number;
  /** 当日单次记录（旧数据文件没有该字段时为空数组） */
  sessions: PomodoroSession[];
}

/** 快照元信息（记录在 md 文件目录段中） */
export interface SnapshotMeta {
  id: string;
  version: number;
  label: string;
  created_at: string;
}
