# 花钟数据规范 v2（花笺 / 花钟 共用契约）

> 本文件是**唯一契约**。花笺（`/mnt/data1/写作和文件/花笺/花笺无限/花笺infinity` 与 `.../花笺infinity-win`）
> 与花钟（`/mnt/data1/写作和文件/花钟`）的所有实现必须严格遵守；任何偏离先改本文件。

## 1. 目标

把番茄钟用户数据从花笺的用户数据里彻底分离，使花钟将来能作为独立 app 运行；
两个 app 可能**同时运行并同时计时**，写同一份数据文件，必须做到：

1. 两边各自完成的番茄记录**都保留**，互不覆盖；
2. 当天时长按**区间并集**统计：重叠部分只算一次，即"开始最早 → 结束最晚"；
3. 用户可**删除任意记录**（任何时候），可**缩短**已完成番茄的时长，**不得加长**，**不得手工新增**；
4. 数据文件可**整体拷贝到其他设备/系统**直接使用。

## 2. 目录布局

根目录由系统配置目录推导（不是 app 私有目录，**与 app 标识符无关**）：

```
<config_dir>/huajian/                 # Linux: ~/.config/huajian
│                                     # Windows: %APPDATA%\huajian
│                                     # macOS: ~/Library/Application Support/huajian
├── 花笺/
│   └── settings.json                 # 花笺自己的设置（不含番茄钟设置）
└── 花钟.hz                           # 花钟全部用户数据（单文件，tar 套 json）
```

- 临时文件：`花钟.hz.tmp`（写入中）、`花钟.hz.lock`（互斥锁）。正常退出后两者都不该残留。
- 旧版位置：`<config_dir>/com.huajian.app/`（只读，仅用于一次性导入，见第 6 节）。

## 3. `花钟.hz` 文件格式

- **无压缩 tar**（`tar` crate，`HeaderMode::Deterministic`）。
- 只含一个条目：`huazhong.json`（UTF-8、`serde_json::to_vec_pretty`）。
- tar header：`mode = 0o644`、`mtime = 0`、uid/gid = 0、类型普通文件。保证同样内容产出同样字节。
  （`mtime=0` 指 header 字段为 0；`tar -tvf` 会按本机时区把 0 显示成 `1970-01-01 08:00` 之类，属正常。）
- 写入必须：先写 `花钟.hz.tmp` → `fs::rename` 覆盖 `花钟.hz`（原子替换）。

```json
{
  "format": 2,
  "app": "huazhong",
  "settings": { "mode": "standard", "focus_minutes": 25, "break_minutes": 5, "loops": 4 },
  "days": [
    {
      "date": "2026-02-10",
      "baseline": { "standard_seconds": 0, "standard_count": 0, "forward_seconds": 0, "forward_count": 0 },
      "standard_count": 2,
      "forward_count": 0,
      "standard_seconds": 1620,
      "forward_seconds": 0,
      "sessions": [
        { "id": "3f1c…", "at": "09:25", "seconds": 1500, "mode": "standard",
          "start": "09:00:32", "end": "09:25:32" }
      ]
    }
  ]
}
```

字段规则：

| 字段 | 规则 |
|---|---|
| `format` | 恒为 `2`。读取时若缺失或 > 2：按 2 解析（尽力而为），未知字段忽略；**任何一次写入都会把 `format` 归一化为 2**，即未来 v3 字段在 v2 实现写入后会丢失——因此升级格式必须先改本契约并同步三个实现 |
| `settings.mode` | `standard` \| `forward`，其他值回落到 `standard` |
| `settings.focus_minutes` | 夹取到 `1..=180` |
| `settings.break_minutes` | 夹取到 `0..=60` |
| `settings.loops` | 夹取到 `1..=12` |
| `days[].date` | 本地日期 `YYYY-MM-DD`；`days` 按日期**升序** |
| `days[].sessions[].id` | uuid v4 字符串；缺失时读取阶段补发 |
| `days[].sessions[].at` | 完成时刻 `HH:MM`（展示用；由 `end` 派生） |
| `days[].sessions[].seconds` | 时长秒；合法区间 `1..=86400`，越界记录**丢弃** |
| `days[].sessions[].mode` | `standard` \| `forward`，其他值按 `standard` 计 |
| `days[].sessions[].start` / `end` | 本地墙上时钟 `HH:MM:SS`；完成时 `end = 当前本地时刻`，`start = end − seconds` |
| `days[].baseline` | **仅**旧版"只有合计、没有明细"的日期使用；四舍五入前的原始合计。新记录一律为 0，为 0 时可省略 |
| `standard_count` / `forward_count` | **派生缓存**：`baseline 计数 + 明细条数` |
| `standard_seconds` / `forward_seconds` | **派生缓存**：`baseline 秒 + 该模式下明细的并集秒`（跨模式重叠不在此体现） |

**读取时合计一律重算**（`baseline + 明细并集`），文件里的派生缓存即使被手改也不会造成错账；
**写入时**把重算结果写回这两个缓存字段（让旧版本也能读到近似值）。

### 3.1 时间与并集算法

- 时间统一按"本地墙上时钟"处理，**不存时区/时间戳**，保证跨设备拷贝后显示的是记录当时的钟点。
- `hms("HH:MM:SS") → 0..86399`；`at` 缺失时分从 `end` 派生。
- 单条区间：`[start_sec, end_sec]`；若 `start_sec > end_sec`（跨午夜的正向番茄），把 `start_sec -= 86400`。
- 并集：所有区间裁剪到当天 `[0, 86400]`，丢弃 `end <= start` 的空区间；按起点排序后合并，累加长度。
- 当天总计秒 = `baseline.standard_seconds + baseline.forward_seconds + 并集(全部明细)`（**整体并集，不分模式**）。
- 当天番茄个数 = `standard_count + forward_count`（**按原始条数**，两边同时计时就是 2 个）。
- 分模式时长仅作缓存/兼容，界面**不再展示**分项时长，只展示分项次数。

### 3.2 容错

- 文件缺失 / 不是合法 tar / 没有 `huazhong.json` / JSON 解析失败 → 返回**空数据 + 默认设置**，**绝不 panic**，
  且**不得**因此覆盖原文件（只有真正的写入才落盘）。
- 缺少 `sessions`、`at`、`start`、`end`、`id` 等字段 → 按上表补齐/派生。
- 同一天重复出现多条同 id 记录 → 保留第一条。

### 3.3 边界裁决（实现必须与本节一致）

- 不满足 `YYYY-MM-DD` 结构的 `days[].date`（例如 `2026-2-1`）→ **整条日期静默丢弃**，不猜测、不修复。
- 同一天**同时**存在 `sessions` 与顶层合计字段（旧版残留）→ **以 `sessions` 为准**：顶层合计丢弃，
  `baseline` 只在 `sessions` 为空时才从顶层合计取值（避免重复计数）。这条同时覆盖路径与迁移。
- 跨午夜记录（`start > end`）：`seconds` 保留原值不拆分，**当天合计只计午夜之后的部分**（§3.1 的裁剪规则）；
  记录仍归属"完成日"，前半夜的份额不折算到前一天。界面口径见 §8。

## 4. 并发写（花笺与花钟同时运行）

**互斥**（手写锁，不引入新 crate）：

1. 取锁：`花钟.hz.lock` 用 `OpenOptions::new().create_new(true)` 抢占；
2. 失败则每 50ms 重试，最多 2 秒；
3. 若锁文件 mtime 早于当前时间 10 秒以上，视为陈旧锁（进程崩溃残留），删除后重试；
4. 自旋期间累计 2 秒仍拿不到 → 返回错误"数据文件正被占用，请稍后重试"。

**每次变更都必须走这个流程**：

```
取锁 → 从磁盘重新 load（拿到对方刚写入的记录）→ 在内存里应用本次变更 → 重算派生合计 → 原子写盘 → 释放锁
```

- 释放锁：RAII guard，正常/异常路径都删除锁文件。
- 只读操作（打开面板）可直接 `load`，不需要锁（rename 是原子的）。
- 持有期间**不刷新锁文件 mtime**：单次事务必须远短于 10 秒；若某次事务超过 10 秒，
  锁在自旋方眼里会变成"陈旧"并被抢占（已知取舍，实现不要刷新 mtime，否则崩溃残留永远无法自愈）。
- **抢锁失败的一方不得删除对方的锁文件**；只有判定为陈旧锁时才允许 `remove_file` 后重试。

## 5. 记录管理规则（两个 app 一致）

| 操作 | 允许 | 说明 |
|---|---|---|
| 删除记录 `delete(date, id)` | ✅ | 任意日期任意记录；删除后重算合计 |
| 缩短时长 `shorten(date, id, seconds)` | ✅ | 要求 `1 ≤ seconds ≤ 原 seconds`；`end` 不变，`start = end − seconds`，`mode` 不变 |
| 加长 / 手工新增 | ❌ | 后端不提供新增接口；缩短接口对超限值返回错误"只能减短，不能加长" |
| 修改 `baseline` 旧汇总 | ❌ | 没有明细对象，界面只展示、不可编辑删除 |

"不得加长、不得手工新增"由**命令层（§7）与界面（§8）**保证；数据层的 `save(root, data)` 是
**覆盖式内部 API**（仅供迁移首次落盘与测试使用），不受该约束，也不得被命令层暴露给界面。

## 6. 旧数据一次性导入（方案 A）

启动时执行，**幂等**（新文件已存在就整体跳过）：

1. 若 `<root>/花笺/settings.json` 不存在，且 `<legacy>/settings.json` 存在：
   - 读取旧设置（`theme`/`dark`/`sidebar_width`/`width_mode`/`full_width`/`boxes`），**丢弃 `pomodoro` 字段**，写入新位置；
   - 旧文件改名为 `settings.json.migrated-YYYYMMDD`（同目录，不删除；重名则追加 `-2`、`-3`…）。
   （迁移只**原样搬运**旧设置 JSON；`full_width → width_mode` 的归一化仍由花笺读取设置时完成，不在这里做。）
2. 若 `<root>/花钟.hz` 不存在，且 `<legacy>/settings.json` 或 `<legacy>/pomodoro_stats.json` 存在：
   - `settings` 取自旧 `settings.json` 的 `pomodoro`（缺失则默认值），按第 3 节夹取；
   - `days` 取自旧 `pomodoro_stats.json`：
     - 有 `sessions` 的日期：逐条升级 → 补 `id`、`at` 缺秒补 `00`、`end = at`、派生 `start = end − seconds`；
     - **只有合计、没有明细**的日期：整日合计写入 `baseline`，`sessions` 为空；
   - 写入 `花钟.hz`；
   - 旧 `pomodoro_stats.json`（若存在）改名为 `pomodoro_stats.json.migrated-YYYYMMDD`。
3. 发生了任何导入 → 启动提示（见第 7 节命令）返回一次 `已把用户数据迁移到 <root>`。
4. **原子性（两阶段）**：必须**先把两份旧文件全部读入并解析校验成功**，之后才允许开始写新文件与改名。
   任何解析/读取失败都发生在阶段一 → **一份旧文件都不改名、一个新文件都不落盘**，返回错误提示、
   本次以默认值继续运行，下次启动重试。阶段二（写新文件 + 改名归档）中途的 I/O 失败允许留下
   "新文件已存在但旧文件未改名"的部分状态，但**绝不允许**旧文件在解析失败时被改名。

旧版字段兼容（长期保留，不需要导入也成立）：
- 旧 `settings.json` 缺字段 → 默认值；`full_width` → `width_mode` 迁移规则照旧。
- 旧 `pomodoro_stats.json` 的 `at` 只有 `HH:MM`、`sessions` 缺省、没有 `baseline`/`id`/`start`/`end` 都要能读。

## 7. 后端命令契约（花笺与花钟同名同参）

| 命令 | 参数 | 返回 | 说明 |
|---|---|---|---|
| `get_pomodoro_settings` | — | `PomodoroSettings` | 夹取后的设置 |
| `set_pomodoro_settings` | `mode, focus_minutes, break_minutes, loops` | `()` | 走第 4 节流程写 `.hz` |
| `record_pomodoro` | `seconds, mode` | `()` | `end = 当前本地时刻（秒级）`，`start = end − seconds`；秒数非法（0 或 > 86400）返回错误 |
| `get_pomodoro_stats` | `year, month` | `Vec<PomodoroDayStats>` | 只返回该月有记录的日期，合计为重算值，`sessions` 带 `id/start/end` |
| `delete_pomodoro_session` | `date, id` | `()` | 找不到记录返回错误 |
| `shorten_pomodoro_session` | `date, id, seconds` | `()` | 越界返回错误"只能减短，不能加长" |
| `huazhong_path` | — | `String` | 供"打开数据位置"：`花钟.hz` 存在时返回它的绝对路径；尚不存在时返回数据根目录 `<root>`（必要时创建目录），保证按钮不会静默失败 |
| `get_startup_notice` | — | `Option<String>` | 一次性提示（迁移成功/失败），读取后清空 |

花笺另有（保持现状）：`get_settings`（**不再含 `pomodoro` 字段**）、`set_appearance`、`reorder_boxes`、`reveal_in_folder`、
花匣/花笺/快照/导出等命令。花钟不需要花笺的那一套。

状态管理：进程内持有 `Mutex<HuazhongData>`（启动时 load）。任何变更命令走第 4 节流程，
写完把磁盘结果同步回内存状态。

## 8. 界面行为（花笺；花钟的最小版见第 9 节）

- 番茄设置读写 `.hz`（不再读 `settings.json` 的 `pomodoro`）。
- 番茄记录面板：
  - 明细行 = `完成时刻 | 模式 | 时长（可点击改短） | 删除`；
  - 与另一条记录时间区间相交的行显示「重叠」小标记（合计已按并集去重，行内可加"不重复计时"说明）；
  - 删除：行内二次确认；
  - 改短：行内数字输入框，单位**分钟（整数）**，`1 ≤ 值 ≤ floor(原秒 / 60)`；超上限 → 夹回上限并提示
    「只能减短，不能加长」；小于 1 分钟或原时长 < 60 秒 → 拒绝/禁用并提示；提交后刷新当月；
  - 旧汇总日（`sessions` 为空但 `count > 0`）→ 显示「旧版本汇总 · 无明细」+ 合计时长，无编辑入口；
  - 面板底部「打开数据位置」→ `reveal_in_folder(huazhong_path())`。
- 启动时调用一次 `get_startup_notice`，有内容则用状态栏提示 8 秒后清空。
- 跨午夜记录：明细行显示记录自身的 `seconds`（不拆分、不改写），当天合计只计午夜之后的部分；
  两者不一致属**既定口径**，界面不为此改写数据。

## 9. 花钟 app（最小可用）

- 技术栈与花笺一致：Rust + Svelte 5 + Vite + Tauri 2；目录 `/mnt/data1/写作和文件/花钟`，git 管理。
- 标识符 `com.huajian.huazhong`、`productName`/`mainBinaryName` = `huazhong`、窗口标题「花钟」、版本 `0.1.0`；
  dev 端口用 `1430`（避免与花笺 1420 冲突）。
- 复用同一份数据层代码：`src-tauri/src/paths.rs`、`src-tauri/src/huazhong.rs`
  （与花笺的对应文件**逐字节相同**，改动必须两边同步；见 `AGENTS.md`）。
- 功能：标准/正向计时（沿用花笺 `pomodoro-core.ts` 的状态机，复制一份）、设置、今日明细（删除 + 改短）、
  本月合计、状态提示、「打开数据位置」。不做花匣/笔记/主题切换/日历热力图。
- 依赖只允许用花笺已有的那批（`tauri`、`tauri-plugin-dialog`、`tauri-plugin-opener`、`serde`、`serde_json`、
  `tar`、`uuid`、`chrono`、`base64`），**不新增 crate**。

## 10. 验收

- 花笺（两个仓库）：`cd src-tauri && cargo test` 全绿；`npm run check` 全绿；`npm run build` 成功。
- 花钟：`cargo test`（数据层）与 `cargo check` 通过；`npm run build` 成功。
- 数据层测试必须覆盖：tar 往返、并集（含重叠、相邻、跨午夜）、次数口径、删除、缩短、拒绝加长、
  非法秒数丢弃、旧格式（无 id/start/end/只有合计）导入、并发追加（两个进程都不丢记录）、
  损坏文件回默认且不覆盖原文件、**迁移两阶段原子性**（旧 stats 损坏时**两份旧文件都不改名、新文件都不落盘**）。
- 手工验收：`tar -tf <root>/花钟.hz` 输出 `huazhong.json`；把 `.hz` 拷到 `/tmp` 用系统 `tar` 与
  `python3 -m json.tool`（本机没有 jq）能读出完整 JSON；用假 HOME 启动一次花笺，确认旧数据被导入、
  旧文件被归档为 `*.migrated-YYYYMMDD`、第二次启动零改动。