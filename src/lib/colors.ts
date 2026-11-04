/** 花笺调色盘：暖纸色调，与花笺主题契合 */
export const PALETTE = [
  '#b4753f', // 赭石
  '#c0392b', // 朱砂
  '#d9822b', // 琥珀
  '#b8860b', // 鎏金
  '#5b8c5a', // 竹青
  '#3a7d9e', // 黛蓝
  '#4f6d9e', // 青黛
  '#7d5ba6', // 紫藤
  '#a65b8c', // 玫紫
  '#9e6b5b', // 茶褐
  '#6b7280', // 铅灰
  '#8a7b5d', // 苔绿
];

export function nextColor(used: string[]): string {
  for (const c of PALETTE) {
    if (!used.includes(c)) return c;
  }
  return PALETTE[Math.floor(Math.random() * PALETTE.length)];
}
