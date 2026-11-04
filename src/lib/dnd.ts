/**
 * 侧边栏指针拖拽共享设施。
 *
 * 背景：Windows 上 WebView2 在 tauri dragDropEnabled（「拖 .hxl 进窗口打开」依赖它）开启时
 * 会接管拖拽并禁用页面内 HTML5 Drag & Drop API，导致侧边栏拖拽排序在 Windows 不可用。
 * 因此统一改用指针事件（pointerdown/move/up）自行实现拖拽，全平台行为一致，
 * 且不影响 tauri 的文件拖入。
 *
 * 关键不变量：`claimDrag` 拿到的所有权**必须**由 `releaseDrag` 归还。
 * 调用方不要用「有没有 ghost」来判断会话是否开启（取不到行时 ghost 为空），
 * 而要用自己的会话标记（`dragIdx !== -1` / `dragNoteId !== null`），否则一旦漏归还，
 * 整个应用的拖拽会永久失效直到重启。
 */

let dragOwner: object | null = null;
let clickSuppressedUntil = 0;

/** 申请拖拽权（同一时刻只允许一个拖拽会话），成功返回 true */
export function claimDrag(owner: object): boolean {
  if (dragOwner) return false;
  dragOwner = owner;
  return true;
}

/**
 * 释放拖拽权。
 *
 * @param suppress 拖拽结束后是否抑制随后 250ms 内的 click。
 *   - 提交型结束（松手落点）传 true：那次 click 属于拖拽误触发，应吞掉；
 *   - 取消型结束（Esc / pointercancel）传 false：用户紧接着的正常点击不该被吞。
 */
export function releaseDrag(owner: object, suppress = true): void {
  if (dragOwner === owner) dragOwner = null;
  if (suppress) clickSuppressedUntil = Date.now() + 250;
}

/** 拖拽刚结束后的 click 应被忽略（避免拖完触发打开花笺/折叠花匣） */
export function clickSuppressed(): boolean {
  return Date.now() < clickSuppressedUntil;
}

/** 克隆一个元素作为拖拽幽灵，挂在 body 下（移除其中的按钮与拖拽状态类） */
export function makeGhost(src: HTMLElement): HTMLElement {
  const ghost = src.cloneNode(true) as HTMLElement;
  ghost.classList.remove('dragging', 'drop-above', 'drop-below', 'drag-over', 'active');
  ghost.querySelectorAll('button').forEach((b) => b.remove());
  ghost.classList.add('dnd-ghost');
  ghost.style.pointerEvents = 'none';
  document.body.appendChild(ghost);
  return ghost;
}

export function removeGhost(ghost: HTMLElement | null): void {
  ghost?.remove();
}

/** 按抓取偏移把幽灵定位到指针处 */
export function positionGhost(
  ghost: HTMLElement,
  clientX: number,
  clientY: number,
  offX: number,
  offY: number,
): void {
  ghost.style.left = `${clientX - offX}px`;
  ghost.style.top = `${clientY - offY}px`;
}

/**
 * 指针靠近可滚动容器上下边缘时按距离自动滚动。
 *
 * 必须传入**真正可滚动**的元素：侧栏里滚动的是 `.box-list`，而不是花笺列表的包裹层
 * （后者的高度由内容撑开，对它设置 scrollTop 没有任何效果）。
 */
export function nudgeScrollContainer(
  el: HTMLElement | null,
  clientY: number,
  edge = 28,
): void {
  if (!el) return;
  const rect = el.getBoundingClientRect();
  if (clientY < rect.top + edge) {
    el.scrollTop -= Math.max(4, (rect.top + edge - clientY) / 3);
  } else if (clientY > rect.bottom - edge) {
    el.scrollTop += Math.max(4, (clientY - (rect.bottom - edge)) / 3);
  }
}
