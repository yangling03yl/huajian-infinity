/**
 * 保存协调器。
 *
 * 编辑器的自动保存是 1.2s 去抖，而「关闭花匣」「关闭窗口」这类操作会立刻拆掉编辑器，
 * 于是最后一段输入来不及落盘。这里把「当前编辑上下文的 flush」注册成一个可外部调用的
 * 单点，供两处复用：
 * - 关闭花匣（`stores.removeBoxUI`）在调 `close_box` 之前先 flush；
 * - 关闭窗口时 Rust 侧发 `flush-request` 事件，前端 flush 完再 `flush_ack`。
 *
 * 同一时刻只会有一个编辑器，因此只保留最后一次注册；注销时校验身份，避免旧组件
 * 的卸载把新组件的注册清掉。
 */

type FlushFn = () => Promise<void>;

let current: FlushFn | null = null;

/** 注册当前编辑上下文的保存函数，返回注销函数 */
export function registerFlush(fn: FlushFn): () => void {
  current = fn;
  return () => {
    if (current === fn) current = null;
  };
}

/** 立刻保存当前编辑内容；没有注册（如未打开花笺）时立即 resolve */
export async function flushNow(): Promise<void> {
  const fn = current;
  if (!fn) return;
  try {
    await fn();
  } catch {
    // 保存失败已在状态栏提示；这里绝不能抛，否则会阻断关窗/关花匣流程
  }
}
