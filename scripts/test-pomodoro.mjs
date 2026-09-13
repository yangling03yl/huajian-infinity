// 番茄钟状态机验证：标准/正向计时、循环与休息、暂停、追帧与防死循环
// 用法: node scripts/test-pomodoro.mjs
// 说明: 只依赖 pomodoro-core.ts（纯逻辑，不含 Svelte/Tauri），由 Node 直接做类型擦除执行
import assert from 'node:assert/strict';
import {
  DEFAULT_SETTINGS,
  MAX_FORWARD_SECONDS,
  advance,
  announcement,
  clampSettings,
  elapsedSeconds,
  finishForward,
  formatClock,
  idleState,
  pauseSession,
  resetPhase,
  resumeSession,
  skipBreak,
  startSession,
  stopSession,
} from '../src/lib/pomodoro-core.ts';

const MIN = 60_000;

let passed = 0;
let failed = 0;

function t(name, fn) {
  try {
    fn();
    passed += 1;
    console.log(`  ✓ ${name}`);
  } catch (e) {
    failed += 1;
    console.error(`  ✗ ${name}\n    ${e.message}`);
  }
}

/** 默认：1 分钟番茄 / 1 分钟休息 / 2 个番茄 */
function cfg(over = {}) {
  return { mode: 'standard', focus_minutes: 1, break_minutes: 1, loops: 2, ...over };
}

console.log('番茄钟状态机自测');

t('标准模式：专注→休息→专注→结束', () => {
  const t0 = 1_000_000;
  let r = advance(startSession(idleState(cfg()), t0), t0 + 30_000);
  assert.equal(r.records.length, 0, '未到终点不应有记录');
  assert.equal(r.state.phase, 'focus');

  r = advance(r.state, t0 + MIN);
  assert.deepEqual(r.records, [{ seconds: 60, mode: 'standard' }]);
  assert.equal(r.state.phase, 'break');
  assert.equal(r.state.completed, 1);

  r = advance(r.state, t0 + 2 * MIN);
  assert.equal(r.state.phase, 'focus');
  assert.equal(r.state.endsAt, t0 + 3 * MIN);

  r = advance(r.state, t0 + 3 * MIN);
  assert.equal(r.records.length, 1);
  assert.equal(r.state.phase, 'idle');
  assert.equal(r.state.completed, 2);
  assert.equal(r.state.finished, true);
  assert.equal(r.justFinished, true);
});

t('未到终点不推进，且保持同一引用（避免无谓渲染）', () => {
  const t0 = 1_100_000;
  const s = startSession(idleState(cfg()), t0);
  const r = advance(s, t0 + MIN - 1);
  assert.equal(r.phaseChanged, false);
  assert.equal(r.state, s);
});

t('迟到很久的 tick：按配置时长记录，休息从番茄结束时刻起算', () => {
  const t0 = 1_200_000;
  const s = startSession(idleState(cfg({ loops: 2 })), t0);
  const r = advance(s, t0 + 90_000);
  assert.equal(r.records[0].seconds, 60);
  assert.equal(r.state.phase, 'break');
  assert.equal(r.state.endsAt, t0 + 2 * MIN);
});

t('追帧：最小化 10 分钟后一次性补推 4 个番茄', () => {
  const t0 = 2_000_000;
  const s = startSession(idleState(cfg({ loops: 4 })), t0);
  const r = advance(s, t0 + 10 * MIN);
  assert.equal(r.records.length, 4);
  assert.ok(r.records.every((x) => x.seconds === 60 && x.mode === 'standard'));
  assert.equal(r.state.phase, 'idle');
  assert.equal(r.state.completed, 4);
  assert.equal(r.justFinished, true);
});

t('休息 0 分钟：连续专注也能正常结束（不死循环）', () => {
  const t0 = 3_000_000;
  const s = startSession(idleState(cfg({ break_minutes: 0, loops: 3 })), t0);
  const r = advance(s, t0 + 10 * MIN);
  assert.equal(r.records.length, 3);
  assert.equal(r.state.phase, 'idle');
  assert.equal(r.state.completed, 3);
});

t('极端跨度（30 天）：追帧有步数上限，不会卡死', () => {
  const t0 = 3_100_000;
  const s = startSession(idleState(cfg({ break_minutes: 0, loops: 12 })), t0);
  const started = Date.now();
  const r = advance(s, t0 + 30 * 24 * 60 * MIN);
  assert.ok(Date.now() - started < 1000, '应在 1 秒内返回');
  assert.equal(r.state.phase, 'idle');
});

t('正向计时：不会自动结束，手动完成按实际时长记录', () => {
  const t0 = 4_000_000;
  let s = startSession(idleState(cfg({ mode: 'forward', loops: 3 })), t0);
  assert.equal(s.endsAt, null);

  let r = advance(s, t0 + 5 * MIN);
  assert.equal(r.records.length, 0);
  assert.equal(r.state.phase, 'focus');

  r = finishForward(r.state, t0 + 7 * MIN + 30_000);
  assert.deepEqual(r.records, [{ seconds: 450, mode: 'forward' }]);
  assert.equal(r.state.phase, 'break');
  assert.equal(r.state.completed, 1);
});

t('正向计时：不足 1 分钟不计入统计，但仍推进循环', () => {
  const t0 = 5_000_000;
  const s = startSession(idleState(cfg({ mode: 'forward', loops: 2 })), t0);
  const r = finishForward(s, t0 + 30_000);
  assert.equal(r.records.length, 0);
  assert.equal(r.state.completed, 1);
  assert.equal(r.state.phase, 'break');
});

t('正向计时：超过 8 小时自动封顶结算', () => {
  const t0 = 6_000_000;
  const s = startSession(idleState(cfg({ mode: 'forward', loops: 2 })), t0);
  const r = advance(s, t0 + (MAX_FORWARD_SECONDS + 60) * 1000);
  assert.deepEqual(r.records, [{ seconds: MAX_FORWARD_SECONDS, mode: 'forward' }]);
  assert.equal(r.state.phase, 'break');
});

t('暂停/继续：已进行时长不漂移', () => {
  const t0 = 7_000_000;
  let s = startSession(idleState(cfg()), t0);
  s = pauseSession(s, t0 + 20_000);
  assert.equal(s.paused, true);
  assert.equal(elapsedSeconds(s, t0 + 10 * MIN), 20);

  const frozen = advance(s, t0 + 10 * MIN);
  assert.equal(frozen.records.length, 0);
  assert.equal(frozen.state.paused, true);

  s = resumeSession(s, t0 + 10 * MIN);
  assert.equal(s.endsAt, t0 + 10 * MIN + 40_000);

  const r = advance(s, s.endsAt);
  assert.deepEqual(r.records, [{ seconds: 60, mode: 'standard' }]);
});

t('跳过休息 / 重置当前阶段', () => {
  const t0 = 8_000_000;
  let s = advance(startSession(idleState(cfg()), t0), t0 + MIN).state;
  assert.equal(s.phase, 'break');

  s = skipBreak(s, t0 + MIN + 5_000);
  assert.equal(s.phase, 'focus');
  assert.equal(s.endsAt, t0 + MIN + 5_000 + MIN);

  const reset = resetPhase(s, t0 + MIN + 20_000);
  assert.equal(reset.phase, 'focus');
  assert.equal(reset.completed, 1, '重置不应改变已完成个数');
  assert.equal(reset.endsAt, t0 + MIN + 20_000 + MIN);
});

t('停止：回到待开始并清零进度', () => {
  const t0 = 9_000_000;
  const running = advance(startSession(idleState(cfg()), t0), t0 + MIN).state;
  const stopped = stopSession(running);
  assert.equal(stopped.phase, 'idle');
  assert.equal(stopped.completed, 0);
  assert.equal(stopped.finished, false);
});

t('运行中改设置：当前阶段不受影响，下一阶段生效', () => {
  const t0 = 10_000_000;
  const started = startSession(idleState(cfg({ loops: 3 })), t0);
  const changed = { ...started, settings: { ...started.settings, focus_minutes: 5, break_minutes: 2 } };

  let r = advance(changed, t0 + MIN);
  assert.equal(r.records[0].seconds, 60, '当前番茄仍按 1 分钟结算');
  assert.equal(r.state.endsAt, t0 + MIN + 2 * MIN, '休息用新值 2 分钟');

  r = advance(r.state, t0 + 3 * MIN);
  assert.equal(r.state.phase, 'focus');
  assert.equal(r.state.endsAt, t0 + 3 * MIN + 5 * MIN, '下一个番茄用新值 5 分钟');
});

t('设置夹取：越界夹取、非法值回落默认', () => {
  assert.deepEqual(clampSettings({ mode: 'x', focus_minutes: 0, break_minutes: 999, loops: 0 }), {
    mode: 'standard',
    focus_minutes: 1,
    break_minutes: 60,
    loops: 1,
  });
  assert.deepEqual(
    clampSettings({ mode: 'forward', focus_minutes: 500, break_minutes: 0, loops: 99 }),
    { mode: 'forward', focus_minutes: 180, break_minutes: 0, loops: 12 },
  );
  assert.deepEqual(
    clampSettings({ mode: 'standard', focus_minutes: NaN, break_minutes: NaN, loops: NaN }),
    DEFAULT_SETTINGS,
  );
});

t('提示文案：区分「番茄结束」与「休息结束」', () => {
  const t0 = 14_000_000;

  // 正向计时不足 1 分钟就结束：进入了休息，但这是「番茄结束」而非「休息结束」
  const short = finishForward(
    startSession(idleState(cfg({ mode: 'forward', loops: 2 })), t0),
    t0 + 30_000,
  );
  assert.equal(short.records.length, 0);
  assert.equal(short.state.phase, 'break');
  assert.deepEqual(announcement(short), { text: '本次番茄不足 1 分钟，未计入统计', beeps: 1 });

  // 标准模式：番茄完成进入休息
  const done = advance(
    startSession(idleState(cfg({ break_minutes: 5, loops: 2 })), t0),
    t0 + MIN,
  );
  assert.deepEqual(announcement(done), { text: '番茄完成，休息 5 分钟', beeps: 2 });

  // 休息 0 分钟：番茄完成直接开始下一个
  const noBreak = advance(
    startSession(idleState(cfg({ break_minutes: 0, loops: 2 })), t0),
    t0 + MIN,
  );
  assert.deepEqual(announcement(noBreak), { text: '番茄完成，开始第 2/2 个', beeps: 2 });

  // 休息结束：开始下一个番茄
  const next = advance(done.state, t0 + MIN + 5 * MIN);
  assert.deepEqual(announcement(next), { text: '休息结束，开始第 2/2 个番茄', beeps: 1 });

  // 最后一个番茄完成
  const last = advance(next.state, next.state.endsAt);
  assert.deepEqual(announcement(last), { text: '本轮番茄钟已完成（2/2）', beeps: 3 });

  // 没有变化时不打扰用户
  const idle = advance(startSession(idleState(cfg()), t0), t0 + 1000);
  assert.equal(announcement(idle), null);
});

t('时间格式化', () => {
  assert.equal(formatClock(0), '00:00');
  assert.equal(formatClock(59), '00:59');
  assert.equal(formatClock(60), '01:00');
  assert.equal(formatClock(3661), '1:01:01');
  assert.equal(formatClock(59.2), '01:00');
  assert.equal(formatClock(-5), '00:00');
});

console.log(`\n通过 ${passed} 项，失败 ${failed} 项`);
if (failed > 0) process.exit(1);
