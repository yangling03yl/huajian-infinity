//! 花笺 md 文件分段格式：正文 / 快照
//!
//! 文件结构（标记必须独占一行）：
//!
//! ```text
//! <!--hxl:body-->
//! （正文 markdown）
//! <!--hxl:/body-->
//!
//! <!--hxl:snapshots-->
//! <!--hxl:snapshot-start {"id":"…","version":1,"label":"快照 1","created_at":"…"}-->
//! （快照内容 markdown）
//! <!--hxl:snapshot-end-->
//! <!--hxl:/snapshots-->
//! ```
//!
//! # 转义（关键不变量）
//!
//! 正文与快照内容是**用户任意文本**，其中完全可能出现形如 `<!--hxl:/body-->` 的一行。
//! 若不处理，解析会在用户那一行提前截断，造成不可恢复的内容丢失。因此：
//!
//! * `serialize` 在写出前做**行首转义**：某行（忽略行首缩进）以反斜杠 + `<!--hxl:` 构成时，
//!   把其前导反斜杠数量 `k` 改写为 `2k+1`；
//! * `parse` 在读入后做**反向解转义**：`2k+1`（奇数）还原为 `k`。
//!
//! 该规则对任意 `k` 严格可逆（`parse(serialize(x)) == x`），且**旧文件天然兼容**：
//! 旧文件没有转义，行首最多是 `k=0`（不触发解转义），行为与以前完全一致。
//! 唯一的代价是：正文真的以 `\<!--hxl:` 开头时会多写入一个反斜杠（读回时还原，不丢信息）。
//!
//! 转义/解转义只发生在 `serialize`/`parse` 之间，`read_note`/`write_note` 对上层
//! 一律使用**未转义**的原始 markdown，调用方无感知。
//!
//! 兼容性：**首个非空行**不是 `<!--hxl:body-->` 的文件整体视为正文（旧格式）。
//! 只在文件任意位置出现标记就按新格式解析，会让旧笔记正文里偶然出现的标记文本
//! 吞掉它之前的所有内容，因此改为首行判定。

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::models::SnapshotMeta;

const BODY_START: &str = "<!--hxl:body-->";
const BODY_END: &str = "<!--hxl:/body-->";
const SNAPS_START: &str = "<!--hxl:snapshots-->";
const SNAPS_END: &str = "<!--hxl:/snapshots-->";
const SNAP_START: &str = "<!--hxl:snapshot-start";
const SNAP_END: &str = "<!--hxl:snapshot-end-->";

/// 所有结构标记的公共前缀：行首（忽略缩进与反斜杠后）出现它就必须转义
const MARKER_PREFIX: &str = "<!--hxl:";
/// 转义符
const ESCAPE: char = '\\';

/// 单个快照：元信息 + 内容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotDoc {
    pub meta: SnapshotMeta,
    pub content: String,
}

/// 一篇花笺的完整文档：正文 + 快照列表
#[derive(Debug, Clone, Default)]
pub struct NoteDoc {
    pub body: String,
    pub snapshots: Vec<SnapshotDoc>,
}

fn normalize(raw: &str) -> String {
    raw.replace("\r\n", "\n")
}

/// 拆分「行首缩进」与「其余部分」，返回 (缩进字节长度, 其余)
fn split_indent(line: &str) -> (usize, &str) {
    let rest = line.trim_start_matches([' ', '\t']);
    (line.len() - rest.len(), rest)
}

/// 统计 `rest` 开头的连续反斜杠数量
fn leading_backslashes(rest: &str) -> usize {
    rest.len() - rest.trim_start_matches(ESCAPE).len()
}

/// 行首转义：`(k 个反斜杠)<!--hxl:` → `(2k+1 个反斜杠)<!--hxl:`
fn escape_line(line: &str) -> Cow<'_, str> {
    let (indent_len, rest) = split_indent(line);
    if rest.is_empty() {
        return Cow::Borrowed(line);
    }
    let bs = leading_backslashes(rest);
    if !rest[bs..].starts_with(MARKER_PREFIX) {
        return Cow::Borrowed(line);
    }
    let mut out = String::with_capacity(line.len() + 2);
    out.push_str(&line[..indent_len]);
    for _ in 0..(2 * bs + 1) {
        out.push(ESCAPE);
    }
    out.push_str(&rest[bs..]);
    Cow::Owned(out)
}

/// 行首解转义：`(奇数个反斜杠)<!--hxl:` → `((k-1)/2 个反斜杠)<!--hxl:`
///
/// 只处理奇数个反斜杠：偶数个不可能是本格式写出的（旧文件为 0，天然跳过）。
fn unescape_line(line: &str) -> Cow<'_, str> {
    let (indent_len, rest) = split_indent(line);
    if rest.is_empty() {
        return Cow::Borrowed(line);
    }
    let bs = leading_backslashes(rest);
    if bs == 0 || bs % 2 == 0 || !rest[bs..].starts_with(MARKER_PREFIX) {
        return Cow::Borrowed(line);
    }
    let mut out = String::with_capacity(line.len());
    out.push_str(&line[..indent_len]);
    for _ in 0..((bs - 1) / 2) {
        out.push(ESCAPE);
    }
    out.push_str(&rest[bs..]);
    Cow::Owned(out)
}

/// 按行做变换后重新拼接（`split('\n')` 保留末尾空行，拼回后与原文逐字节等价）
fn map_lines(text: &str, f: fn(&str) -> Cow<'_, str>) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&f(line));
    }
    out
}

/// 解析 md 文件为正文 + 快照列表；旧格式整体视为正文
pub fn parse(raw: &str) -> NoteDoc {
    let text = normalize(raw);
    let lines: Vec<&str> = text.split('\n').collect();

    // 结构判定：首个非空行必须正好是正文起始标记，否则整体按旧格式（纯正文）处理
    let Some(first) = lines.iter().position(|l| !l.trim().is_empty()) else {
        // 全空白文件：整体当正文，保留原有空行
        return NoteDoc { body: text, snapshots: Vec::new() };
    };
    if lines[first].trim() != BODY_START {
        return NoteDoc { body: text, snapshots: Vec::new() };
    }

    // 缺结束标记时取到文件尾（容错），且不再越界切片
    let body_end = lines
        .iter()
        .enumerate()
        .skip(first + 1)
        .find(|(_, l)| l.trim() == BODY_END)
        .map(|(i, _)| i)
        .unwrap_or(lines.len());
    let body = map_lines(&lines[first + 1..body_end].join("\n"), unescape_line);

    let mut snapshots = Vec::new();
    let after_body = lines.get(body_end + 1..).unwrap_or(&[]);
    if let Some(rel) = after_body.iter().position(|l| l.trim() == SNAPS_START) {
        let ss = body_end + 1 + rel;
        let se = lines
            .iter()
            .enumerate()
            .skip(ss + 1)
            .find(|(_, l)| l.trim() == SNAPS_END)
            .map(|(i, _)| i)
            .unwrap_or(lines.len());
        let mut i = ss + 1;
        while i < se {
            let line = lines[i].trim();
            if let Some(rest) = line.strip_prefix(SNAP_START) {
                // 元信息 JSON 位于 "snapshot-start " 之后、"snapshot-end" 之前
                if let Some(pos) = rest.rfind("-->") {
                    let json = rest[..pos].trim();
                    if let Ok(meta) = serde_json::from_str::<SnapshotMeta>(json) {
                        if let Some(e) = lines
                            .get(i + 1..se)
                            .and_then(|slice| slice.iter().position(|x| x.trim() == SNAP_END))
                            .map(|p| p + i + 1)
                        {
                            let content = map_lines(&lines[i + 1..e].join("\n"), unescape_line);
                            snapshots.push(SnapshotDoc { meta, content });
                            i = e + 1;
                            continue;
                        }
                    }
                }
            }
            i += 1;
        }
    }

    NoteDoc { body, snapshots }
}

/// 序列化为完整分段格式（正文 + 快照）
///
/// 不做 `trim_end`：正文/快照的尾部空白（Markdown 硬换行、空行、缩进）必须原样保留，
/// 否则 `parse(serialize(x)) == x` 不成立。行为尾部补一个换行再写结束标记，
/// 由 `split('\n')` 的空串自然抵消，读回与写入逐字节一致。
pub fn serialize(doc: &NoteDoc) -> String {
    let mut out = String::new();
    out.push_str(BODY_START);
    out.push('\n');
    out.push_str(&map_lines(&normalize(&doc.body), escape_line));
    out.push('\n');
    out.push_str(BODY_END);
    out.push('\n');
    out.push('\n');

    out.push_str(SNAPS_START);
    out.push('\n');
    for snap in &doc.snapshots {
        // SnapshotMeta 全为字符串/整数，序列化不会失败；万一失败宁可跳过，
        // 也不写出一个元信息损坏、解析时会被静默丢弃的坏帧
        let Ok(meta) = serde_json::to_string(&snap.meta) else {
            continue;
        };
        out.push_str(SNAP_START);
        out.push(' ');
        out.push_str(&meta);
        out.push_str("-->\n");
        out.push_str(&map_lines(&normalize(&snap.content), escape_line));
        out.push('\n');
        out.push_str(SNAP_END);
        out.push('\n');
    }
    out.push_str(SNAPS_END);
    out.push('\n');

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(version: u32, label: &str) -> SnapshotMeta {
        SnapshotMeta {
            id: format!("id-{version}"),
            version,
            label: label.into(),
            created_at: format!("2026-08-25T0{version}:00:00Z"),
        }
    }

    #[test]
    fn legacy_file_is_all_body() {
        let doc = parse("# 标题\n\n正文\n");
        assert_eq!(doc.body, "# 标题\n\n正文\n");
        assert!(doc.snapshots.is_empty());
    }

    #[test]
    fn body_only_roundtrip() {
        let doc = NoteDoc {
            body: "# 标题\n\n正文内容 **加粗**\n\n- 列表\n- 第二行".into(),
            snapshots: Vec::new(),
        };
        let s = serialize(&doc);
        assert!(s.contains(BODY_START));
        let parsed = parse(&s);
        assert_eq!(parsed.body, doc.body);
        assert!(parsed.snapshots.is_empty());
    }

    #[test]
    fn snapshot_roundtrip() {
        let mut doc = NoteDoc { body: "正文 v2".into(), snapshots: Vec::new() };
        doc.snapshots.push(SnapshotDoc { meta: meta(1, "快照 1"), content: "正文 v1".into() });
        doc.snapshots.push(SnapshotDoc {
            meta: meta(2, "初稿"),
            content: "# 初稿\n\n第一段\n第二段".into(),
        });
        let s = serialize(&doc);
        let parsed = parse(&s);
        assert_eq!(parsed.body, "正文 v2");
        assert_eq!(parsed.snapshots.len(), 2);
        assert_eq!(parsed.snapshots[0].meta.version, 1);
        assert_eq!(parsed.snapshots[0].content, "正文 v1");
        assert_eq!(parsed.snapshots[1].meta.label, "初稿");
        assert_eq!(parsed.snapshots[1].content, "# 初稿\n\n第一段\n第二段");
        // 快照元信息写在各自的 start 标记里
        assert!(s.contains("\"version\":2"), "{s}");
        assert!(s.contains("\"label\":\"初稿\""), "{s}");
    }

    #[test]
    fn crlf_is_normalized() {
        let doc = NoteDoc { body: "a\nb".into(), snapshots: Vec::new() };
        let s = serialize(&doc);
        // Windows 风格的换行在读取时统一归一化为 \n
        let parsed = parse(&s.replace('\n', "\r\n"));
        assert_eq!(parsed.body, "a\nb");
    }

    #[test]
    fn body_containing_marker_like_text_is_ok() {
        // 正文中间的近似标记（不在行首）不应破坏解析
        let doc = NoteDoc {
            body: "段落一\n\n中间有 <!--hxl:body--> 字样\n\n段落二".into(),
            snapshots: Vec::new(),
        };
        let s = serialize(&doc);
        let parsed = parse(&s);
        assert_eq!(parsed.body, "段落一\n\n中间有 <!--hxl:body--> 字样\n\n段落二");
    }

    #[test]
    fn empty_body_and_empty_snapshots_roundtrip() {
        let doc = NoteDoc::default();
        let s = serialize(&doc);
        let parsed = parse(&s);
        assert_eq!(parsed.body, "");
        assert!(parsed.snapshots.is_empty());
    }

    // ---------- 以下为本次修复新增：标记转义相关 ----------

    /// 正文里出现「独占一行的结束标记」不得截断（修复前会丢光尾部）
    #[test]
    fn body_with_standalone_end_marker_roundtrip() {
        let body = "第一段\n\n<!--hxl:/body-->\n\n第三段";
        let doc = NoteDoc { body: body.into(), snapshots: Vec::new() };
        let parsed = parse(&serialize(&doc));
        assert_eq!(parsed.body, body);
        assert!(parsed.snapshots.is_empty());
    }

    /// 正文里伪造整个快照段，不得解析出幽灵快照
    #[test]
    fn body_cannot_forge_snapshot() {
        let body = "正文\n<!--hxl:/body-->\n<!--hxl:snapshots-->\n\
                    <!--hxl:snapshot-start {\"id\":\"ghost\",\"version\":9,\"label\":\"幽灵\",\"created_at\":\"t\"}-->\n\
                    伪造\n<!--hxl:snapshot-end-->\n<!--hxl:/snapshots-->";
        let doc = NoteDoc { body: body.into(), snapshots: Vec::new() };
        let parsed = parse(&serialize(&doc));
        assert_eq!(parsed.body, body);
        assert!(parsed.snapshots.is_empty(), "正文不应能伪造快照");
    }

    /// 快照内容里出现快照结束标记，不得提前截断、也不得吞掉后续快照
    #[test]
    fn snapshot_with_standalone_marker_roundtrip() {
        let doc = NoteDoc {
            body: "当前正文".into(),
            snapshots: vec![
                SnapshotDoc {
                    meta: meta(1, "快照 1"),
                    content: "开头\n<!--hxl:snapshot-end-->\n结尾".into(),
                },
                SnapshotDoc { meta: meta(2, "快照 2"), content: "第二份".into() },
            ],
        };
        let parsed = parse(&serialize(&doc));
        assert_eq!(parsed.snapshots.len(), 2);
        assert_eq!(parsed.snapshots[0].content, "开头\n<!--hxl:snapshot-end-->\n结尾");
        assert_eq!(parsed.snapshots[1].content, "第二份");
    }

    /// 行首已有 0..4 个反斜杠时，转义/解转义严格互逆
    #[test]
    fn backslash_nesting_roundtrip() {
        for k in 0..5usize {
            let body = format!("{}<!--hxl:/body-->", "\\".repeat(k));
            let doc = NoteDoc { body: body.clone(), snapshots: Vec::new() };
            let parsed = parse(&serialize(&doc));
            assert_eq!(parsed.body, body, "k={k} 时应严格互逆");
        }
        // 缩进 + 反斜杠组合
        let body = "   \\\\<!--hxl:body-->\n正常行".to_string();
        let doc = NoteDoc { body: body.clone(), snapshots: Vec::new() };
        assert_eq!(parse(&serialize(&doc)).body, body);
    }

    /// 旧格式笔记正文里出现标记文本时，标记之前的内容不能被吞掉
    #[test]
    fn legacy_file_with_marker_text_keeps_everything() {
        let raw = "旧笔记第一段\n<!--hxl:body-->\n旧笔记第三段";
        let doc = parse(raw);
        assert_eq!(doc.body, raw);
        assert!(doc.snapshots.is_empty());
    }

    /// 缺 `<!--hxl:/body-->` 时只容错，不得 panic（修复前会越界崩溃）
    #[test]
    fn missing_body_end_does_not_panic() {
        let doc = parse("<!--hxl:body-->\n只有开始标记");
        assert_eq!(doc.body, "只有开始标记");
        assert!(doc.snapshots.is_empty());

        // 只有开始标记、没有正文
        let doc = parse("<!--hxl:body-->");
        assert_eq!(doc.body, "");
    }

    /// 尾部空白（Markdown 硬换行、空行）必须原样保留
    #[test]
    fn trailing_whitespace_preserved() {
        for body in ["第一行  \n第二行\n\n", "末尾两个空格  ", "只有空行\n", ""] {
            let doc = NoteDoc { body: body.into(), snapshots: Vec::new() };
            assert_eq!(parse(&serialize(&doc)).body, body, "{body:?} 应原样保留");
        }
    }

    // ---------- 以下为 0.9.3 兼容性验证 ----------

    /// 0.9.3 的解析算法（任意位置定位起始标记、**无转义**），只用于证明
    /// 新版本写出的文件旧版本仍然读得懂（前向兼容）。
    fn parse_v093(raw: &str) -> (String, Vec<String>) {
        let text = raw.replace("\r\n", "\n");
        let lines: Vec<&str> = text.split('\n').collect();
        let Some(bs) = lines.iter().position(|l| l.trim() == BODY_START) else {
            return (text, Vec::new());
        };
        let be = lines
            .get(bs + 1..)
            .and_then(|s| s.iter().position(|l| l.trim() == BODY_END))
            .map(|i| i + bs + 1)
            .unwrap_or(lines.len());
        let body = lines[bs + 1..be].join("\n");
        let mut snaps = Vec::new();
        if let Some(ss) = lines
            .get(be + 1..)
            .and_then(|s| s.iter().position(|l| l.trim() == SNAPS_START))
            .map(|i| i + be + 1)
        {
            let se = lines
                .get(ss + 1..)
                .and_then(|s| s.iter().position(|l| l.trim() == SNAPS_END))
                .map(|i| i + ss + 1)
                .unwrap_or(lines.len());
            let mut i = ss + 1;
            while i < se {
                if lines[i].trim().starts_with(SNAP_START) {
                    if let Some(e) = lines
                        .get(i + 1..se)
                        .and_then(|s| s.iter().position(|x| x.trim() == SNAP_END))
                        .map(|p| p + i + 1)
                    {
                        snaps.push(lines[i + 1..e].join("\n"));
                        i = e + 1;
                        continue;
                    }
                }
                i += 1;
            }
        }
        (body, snaps)
    }

    /// 0.9.3/更早版本写出的文件（无转义、trim_end、末尾带已废弃的 TOC 段）
    /// 必须被新解析器完整读回 —— 旧花匣直接可打开，不需要任何迁移。
    #[test]
    fn v093_file_is_readable() {
        let raw = concat!(
            "<!--hxl:body-->\n",
            "正文第一段\n",
            "<!--hxl:/body-->\n",
            "\n",
            "<!--hxl:snapshots-->\n",
            "<!--hxl:snapshot-start {\"id\":\"s1\",\"version\":1,\"label\":\"快照 1\",\"created_at\":\"2026-08-25T01:00:00Z\"}-->\n",
            "旧快照内容\n",
            "<!--hxl:snapshot-end-->\n",
            "<!--hxl:/snapshots-->\n",
            "\n",
            "<!--hxl:toc-->\n",
            "{\n  \"version\": 1,\n  \"snapshots\": []\n}\n",
            "<!--hxl:/toc-->\n",
        );
        let doc = parse(raw);
        assert_eq!(doc.body, "正文第一段");
        assert_eq!(doc.snapshots.len(), 1);
        assert_eq!(doc.snapshots[0].meta.label, "快照 1");
        assert_eq!(doc.snapshots[0].content, "旧快照内容");
    }

    /// 反向：新版本写出的文件，0.9.3 的解析算法也要能读（前向兼容）。
    /// 正常内容（不含分帧标记样式文本）逐字节一致。
    #[test]
    fn v093_parser_can_read_new_output() {
        let doc = NoteDoc {
            body: "# 标题\n\n正文 **加粗**\n\n- 列表\n- 第二项".into(),
            snapshots: vec![
                SnapshotDoc { meta: meta(1, "快照 1"), content: "旧内容".into() },
                SnapshotDoc { meta: meta(2, "初稿"), content: "# 初稿\n\n第一段".into() },
            ],
        };
        let s = serialize(&doc);
        let (body, snaps) = parse_v093(&s);
        assert_eq!(body, doc.body);
        assert_eq!(snaps, vec!["旧内容".to_string(), "# 初稿\n\n第一段".to_string()]);
    }

    /// 前向兼容的唯一降级：正文里**真的**出现分帧标记样式文本时，新版本会在行首
    /// 加一个反斜杠，旧版本会把它当普通文本显示（多一个可见的 `\`）；
    /// 但旧版本原样保存后，新版本仍能解转义还原成原文，不丢信息。
    #[test]
    fn marker_text_degrades_gracefully_in_old_version() {
        let body = "开头\n<!--hxl:/body-->\n结尾";
        let doc = NoteDoc { body: body.into(), snapshots: Vec::new() };
        let s = serialize(&doc);
        // 旧版本看到的是多了一个反斜杠的文本，但结构没被破坏（正文完整）
        let (old_body, _) = parse_v093(&s);
        assert_eq!(old_body, "开头\n\\<!--hxl:/body-->\n结尾");
        // 断言：旧版本读到的正文里，那行不会被当成结束标记（否则会被截断）
        assert!(!old_body.ends_with("开头"));
        // 旧版本原样再保存一次，新版本读回仍是原文
        let roundtrip = format!(
            "{BODY_START}\n{old_body}\n{BODY_END}\n\n{SNAPS_START}\n{SNAPS_END}\n"
        );
        assert_eq!(parse(&roundtrip).body, body);
    }

    /// 空文件 / 纯空白文件不产生任何内容
    #[test]
    fn blank_file_is_empty_body() {
        assert_eq!(parse("").body, "");
        assert_eq!(parse("\n\n\n").body, "\n\n\n");
    }

    /// 用固定种子的 LCG 做随机往返测试，不引入外部依赖
    #[test]
    fn fuzz_roundtrip() {
        struct Lcg(u64);
        impl Lcg {
            fn next(&mut self) -> u64 {
                self.0 = self
                    .0
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                self.0
            }
            fn below(&mut self, n: usize) -> usize {
                (self.next() >> 33) as usize % n.max(1)
            }
        }

        const FRAGMENTS: &[&str] = &[
            "<!--hxl:body-->",
            "<!--hxl:/body-->",
            "<!--hxl:snapshots-->",
            "<!--hxl:/snapshots-->",
            "<!--hxl:snapshot-start",
            "<!--hxl:snapshot-end-->",
            "\\<!--hxl:/body-->",
            "\\\\<!--hxl:body-->",
            "   \\<!--hxl:snapshots-->",
            "",
            "正文一段",
            "  缩进行",
            "行尾两个空格  ",
            "# 标题",
            "* 列表",
            "中文标点，符号「」",
        ];

        let mut rng = Lcg(0x5EED_1234_ABCD_0001);
        for round in 0..500 {
            let mut body = String::new();
            for i in 0..rng.below(9) {
                if i > 0 {
                    body.push('\n');
                }
                body.push_str(FRAGMENTS[rng.below(FRAGMENTS.len())]);
            }
            let mut snapshots = Vec::new();
            for v in 0..rng.below(4) as u32 {
                let mut content = String::new();
                for i in 0..rng.below(5) {
                    if i > 0 {
                        content.push('\n');
                    }
                    content.push_str(FRAGMENTS[rng.below(FRAGMENTS.len())]);
                }
                snapshots.push(SnapshotDoc { meta: meta(v + 1, &format!("快照 {}", v + 1)), content });
            }
            let doc = NoteDoc { body, snapshots };
            let parsed = parse(&serialize(&doc));
            assert_eq!(parsed.body, doc.body, "第 {round} 轮正文往返不一致");
            assert_eq!(parsed.snapshots.len(), doc.snapshots.len(), "第 {round} 轮快照数量不一致");
            for (a, b) in parsed.snapshots.iter().zip(doc.snapshots.iter()) {
                assert_eq!(a.meta.id, b.meta.id);
                assert_eq!(a.content, b.content, "第 {round} 轮快照内容往返不一致");
            }
        }
    }
}
