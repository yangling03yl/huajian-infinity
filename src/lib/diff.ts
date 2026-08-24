import { diffLines } from 'diff';

export interface DiffLine {
  type: 'add' | 'del' | 'ctx';
  text: string;
}

export interface DiffStats {
  added: number;
  removed: number;
}

/**
 * 行级差异（快照 → 当前正文）
 * add  = 当前正文相对快照新增的行（绿色）
 * del  = 快照中被删除/修改的行（红色）
 */
export function computeDiff(base: string, current: string): DiffLine[] {
  const parts = diffLines(base, current, { newlineIsToken: false });
  const lines: DiffLine[] = [];
  for (const part of parts) {
    if (part.added) {
      for (const line of part.value.split('\n')) {
        if (line.length > 0) lines.push({ type: 'add', text: line });
      }
    } else if (part.removed) {
      for (const line of part.value.split('\n')) {
        if (line.length > 0) lines.push({ type: 'del', text: line });
      }
    } else {
      for (const line of part.value.split('\n')) {
        if (line.length > 0) lines.push({ type: 'ctx', text: line });
      }
    }
  }
  return lines;
}

export function diffStats(lines: DiffLine[]): DiffStats {
  let added = 0;
  let removed = 0;
  for (const l of lines) {
    if (l.type === 'add') added += 1;
    else if (l.type === 'del') removed += 1;
  }
  return { added, removed };
}
