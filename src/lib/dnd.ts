/**
 * 侧边栏指针拖拽共享设施。
 *
 * 背景：Windows 上 WebView2 在 tauri dragDropEnabled（「拖 .hxl 进窗口打开」依赖它）开启时
 * 会接管拖拽并禁用页面内 HTML5 Drag & Drop API，导致侧边栏拖拽排序在 Windows 不可用。
 * 因此统一改用指针事件（pointerdown/move/up）自行实现拖拽，全平台行为一致，
 * 且不影响 tauri 的文件拖入。
 */

let dragOwner: object | null = null;
let clickSuppressedUntil = 0;

/** 申请拖拽权（同一时刻只允许一个拖拽会话），成功返回 true */
export function claimDrag(owner: object): boolean {
  if (dragOwner) return false;
  dragOwner = owner;
  return true;
}

/** 释放拖拽权；随后短暂窗口内的 click 属于拖拽误触发，调用方应通过 clickSuppressed() 吞掉 */
export function releaseDrag(owner: object): void {
  if (dragOwner === owner) dragOwner = null;
  clickSuppressedUntil = Date.now() + 250;
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
