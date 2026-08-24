//! 花笺 md 文件分段格式：正文 / 快照 / 目录
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
//!
//! <!--hxl:toc-->
//! {"version":1,"snapshots":[…]}
//! <!--hxl:/toc-->
//! ```
//!
//! 兼容性：不含 `<!--hxl:body-->` 标记的旧文件整体视为正文。
//! 读取时以快照段中的元信息为准；目录段在写入时由快照列表生成。

use serde::{Deserialize, Serialize};

use crate::models::SnapshotMeta;

const BODY_START: &str = "<!--hxl:body-->";
const BODY_END: &str = "<!--hxl:/body-->";
const SNAPS_START: &str = "<!--hxl:snapshots-->";
const SNAPS_END: &str = "<!--hxl:/snapshots-->";
const SNAP_START: &str = "<!--hxl:snapshot-start";
const SNAP_END: &str = "<!--hxl:snapshot-end-->";
const TOC_START: &str = "<!--hxl:toc-->";
const TOC_END: &str = "<!--hxl:/toc-->";

/// 目录段 JSON（记录不同快照的版本）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TocDoc {
    pub version: u32,
    #[serde(default)]
    pub snapshots: Vec<SnapshotMeta>,
}

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

/// 解析 md 文件为正文 + 快照列表；旧格式整体视为正文
pub fn parse(raw: &str) -> NoteDoc {
    let text = normalize(raw);
    let lines: Vec<&str> = text.split('\n').collect();

    let body_start = match lines.iter().position(|l| l.trim() == BODY_START) {
        Some(i) => i,
        None => {
            return NoteDoc { body: text, snapshots: Vec::new() };
        }
    };

    // 正文：body 标记到 /body 标记之间；缺失结束标记则取到文件尾（容错）
    let body_end = lines[body_start + 1..]
        .iter()
        .position(|l| l.trim() == BODY_END)
        .map(|i| i + body_start + 1)
        .unwrap_or(lines.len());
    let body = lines[body_start + 1..body_end].join("\n");

    let mut snapshots = Vec::new();
    if let Some(ss) = lines[body_end + 1..]
        .iter()
        .position(|l| l.trim() == SNAPS_START)
        .map(|i| i + body_end + 1)
    {
        let se = lines[ss + 1..]
            .iter()
            .position(|l| l.trim() == SNAPS_END)
            .map(|i| i + ss + 1)
            .unwrap_or(lines.len());
        let mut i = ss + 1;
        while i < se {
            let line = lines[i].trim();
            if let Some(rest) = line.strip_prefix(SNAP_START) {
                // 元信息 JSON 位于 “snapshot-start ” 之后、“-->” 之前
                if let Some(pos) = rest.rfind("-->") {
                    let json = rest[..pos].trim();
                    if let Ok(meta) = serde_json::from_str::<SnapshotMeta>(json) {
                        if let Some(e) = lines[i + 1..se]
                            .iter()
                            .position(|x| x.trim() == SNAP_END)
                            .map(|p| p + i + 1)
                        {
                            let content = lines[i + 1..e].join("\n");
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

/// 序列化为完整分段格式（正文 + 快照 + 目录）
pub fn serialize(doc: &NoteDoc) -> String {
    let mut out = String::new();
    out.push_str(BODY_START);
    out.push('\n');
    out.push_str(doc.body.trim_end());
    out.push('\n');
    out.push_str(BODY_END);
    out.push('\n');
    out.push('\n');

    out.push_str(SNAPS_START);
    out.push('\n');
    for snap in &doc.snapshots {
        let meta = serde_json::to_string(&snap.meta).unwrap_or_else(|_| "{}".into());
        out.push_str(SNAP_START);
        out.push(' ');
        out.push_str(&meta);
        out.push_str("-->\n");
        out.push_str(snap.content.trim_end());
        out.push('\n');
        out.push_str(SNAP_END);
        out.push('\n');
    }
    out.push_str(SNAPS_END);
    out.push('\n');
    out.push('\n');

    let toc = TocDoc {
        version: 1,
        snapshots: doc.snapshots.iter().map(|s| s.meta.clone()).collect(),
    };
    let toc_json = serde_json::to_string_pretty(&toc).unwrap_or_else(|_| "{}".into());
    out.push_str(TOC_START);
    out.push('\n');
    out.push_str(&toc_json);
    out.push('\n');
    out.push_str(TOC_END);
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
        assert!(s.contains(TOC_START));
        let parsed = parse(&s);
        assert_eq!(parsed.body, doc.body);
        assert!(parsed.snapshots.is_empty());
    }

    #[test]
    fn snapshot_roundtrip() {
        let mut doc = NoteDoc {
            body: "正文 v2".into(),
            snapshots: Vec::new(),
        };
        doc.snapshots.push(SnapshotDoc {
            meta: meta(1, "快照 1"),
            content: "正文 v1".into(),
        });
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
        // 目录段记录各快照版本
        assert!(s.contains("\"version\": 2"));
        assert!(s.contains("\"label\": \"初稿\""));
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
        // 正文中间出现的近似标记（不在行首）不应破坏解析
        let doc = NoteDoc {
            body: "段落一\n\n中间有 <!--hxl:body--> 字样\n\n段落二".into(),
            snapshots: Vec::new(),
        };
        let s = serialize(&doc);
        let parsed = parse(&s);
        assert_eq!(
            parsed.body,
            "段落一\n\n中间有 <!--hxl:body--> 字样\n\n段落二"
        );
    }

    #[test]
    fn empty_body_and_empty_snapshots_roundtrip() {
        let doc = NoteDoc::default();
        let s = serialize(&doc);
        let parsed = parse(&s);
        assert_eq!(parsed.body, "");
        assert!(parsed.snapshots.is_empty());
    }
}
