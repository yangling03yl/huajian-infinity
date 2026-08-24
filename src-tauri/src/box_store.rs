use std::collections::HashMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use chrono::Utc;
use tar::{Archive, Builder, Header};
use uuid::Uuid;

use crate::models::{BoxIndex, BoxInfo, NoteMeta, SnapshotMeta};
use crate::note_doc::{self, NoteDoc, SnapshotDoc};

const INDEX_FILE: &str = "box.json";

pub struct OpenBox {
    pub id: String,
    pub path: PathBuf,
    pub index: BoxIndex,
    /// 花匣内的其他文件（box.json 等）
    pub files: HashMap<String, Vec<u8>>,
    /// 各花笺的结构化文档（正文 + 快照），键为 note.file
    pub docs: HashMap<String, NoteDoc>,
}

impl OpenBox {
    pub fn note(&self, note_id: &str) -> Option<&NoteMeta> {
        self.index.notes.iter().find(|n| n.id == note_id)
    }

    fn doc(&self, note_id: &str) -> Result<&NoteDoc, String> {
        let note = self.note(note_id).ok_or("花笺不存在")?;
        self.docs.get(&note.file).ok_or("花笺文件缺失".into())
    }

    fn doc_mut(&mut self, note_id: &str) -> Result<&mut NoteDoc, String> {
        let file = self
            .note(note_id)
            .ok_or("花笺不存在")?
            .file
            .clone();
        self.docs.get_mut(&file).ok_or("花笺文件缺失".into())
    }
}

pub struct BoxStore {
    pub boxes: HashMap<String, OpenBox>,
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn now_epoch() -> u64 {
    Utc::now().timestamp() as u64
}

impl BoxStore {
    pub fn new() -> Self {
        Self { boxes: HashMap::new() }
    }

    /// 打开一个 .hxl 花匣：解析 tar、读取 box.json 索引、笔记解析为正文+快照
    pub fn open(&mut self, path: &Path) -> Result<BoxInfo, String> {
        let bytes = fs::read(path)
            .map_err(|e| format!("读取花匣失败: {e}"))?;
        let mut archive = Archive::new(Cursor::new(bytes));
        let mut files: HashMap<String, Vec<u8>> = HashMap::new();
        let entries = archive
            .entries()
            .map_err(|e| format!("解析花匣失败: {e}"))?;
        for entry in entries {
            let mut entry = entry.map_err(|e| format!("读取花匣条目失败: {e}"))?;
            let name = entry
                .path()
                .map_err(|e| format!("读取花匣条目路径失败: {e}"))?
                .to_string_lossy()
                .into_owned();
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .map_err(|e| format!("读取花匣条目内容失败: {e}"))?;
            files.insert(name, buf);
        }

        let raw = files
            .get(INDEX_FILE)
            .ok_or("不是有效的花匣文件（缺少 box.json）")?;
        let index: BoxIndex =
            serde_json::from_slice(raw).map_err(|e| format!("box.json 解析失败: {e}"))?;

        let mut docs: HashMap<String, NoteDoc> = HashMap::new();
        for note in &index.notes {
            let doc = files
                .remove(&note.file)
                .map(|bytes| note_doc::parse(&String::from_utf8_lossy(&bytes)))
                .unwrap_or_default();
            docs.insert(note.file.clone(), doc);
        }

        let id = Uuid::new_v4().to_string();
        let info = BoxInfo {
            id: id.clone(),
            path: path.display().to_string(),
            name: index.name.clone(),
            note_count: index.notes.len(),
        };
        self.boxes.insert(
            id.clone(),
            OpenBox { id, path: path.to_path_buf(), index, files, docs },
        );
        Ok(info)
    }

    /// 新建一个空花匣并写入磁盘
    pub fn create(&mut self, path: &Path, name: String) -> Result<BoxInfo, String> {
        if path.exists() {
            return Err("目标文件已存在".into());
        }
        let id = Uuid::new_v4().to_string();
        let index = BoxIndex {
            version: 1,
            name,
            created_at: now(),
            notes: Vec::new(),
        };
        let ob = OpenBox {
            id: id.clone(),
            path: path.to_path_buf(),
            index,
            files: HashMap::new(),
            docs: HashMap::new(),
        };
        Self::repack(&ob)?;
        let info = BoxInfo {
            id: id.clone(),
            path: ob.path.display().to_string(),
            name: ob.index.name.clone(),
            note_count: 0,
        };
        self.boxes.insert(id, ob);
        Ok(info)
    }

    pub fn close(&mut self, box_id: &str) -> Result<(), String> {
        self.boxes
            .remove(box_id)
            .map(|_| ())
            .ok_or("花匣未打开".into())
    }

    pub fn get(&self, box_id: &str) -> Result<&OpenBox, String> {
        self.boxes.get(box_id).ok_or("花匣未打开".into())
    }

    pub fn get_mut(&mut self, box_id: &str) -> Result<&mut OpenBox, String> {
        self.boxes.get_mut(box_id).ok_or("花匣未打开".into())
    }

    /// 将花匣全量重打包为 tar（无压缩），临时文件 + 原子重命名
    pub fn repack(bx: &OpenBox) -> Result<(), String> {
        let tmp = bx.path.with_extension("hxl.tmp");
        let file = fs::File::create(&tmp).map_err(|e| format!("创建临时文件失败: {e}"))?;
        let mut builder = Builder::new(file);
        builder.mode(tar::HeaderMode::Deterministic);

        let mut entries: Vec<(String, Vec<u8>)> = bx
            .files
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (file, doc) in &bx.docs {
            entries.push((file.clone(), note_doc::serialize(doc).into_bytes()));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        for (name, bytes) in entries {
            let mut header = Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_mtime(now_epoch());
            header.set_cksum();
            builder
                .append_data(&mut header, &name, bytes.as_slice())
                .map_err(|e| format!("写入花匣失败: {e}"))?;
        }
        builder.finish().map_err(|e| format!("打包花匣失败: {e}"))?;
        fs::rename(&tmp, &bx.path).map_err(|e| format!("保存花匣失败: {e}"))?;
        Ok(())
    }

    pub fn save_index(bx: &mut OpenBox) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(&bx.index).map_err(|e| e.to_string())?;
        bx.files.insert(INDEX_FILE.to_string(), json);
        Self::repack(bx)
    }

    /// 读取正文（不返回快照与目录段）
    pub fn read_note(&self, box_id: &str, note_id: &str) -> Result<String, String> {
        let box_ = self.get(box_id)?;
        Ok(box_.doc(note_id)?.body.clone())
    }

    /// 写入正文（保留快照与目录段）
    pub fn write_note(
        &mut self,
        box_id: &str,
        note_id: &str,
        content: &str,
    ) -> Result<(), String> {
        let box_ = self.get_mut(box_id)?;
        {
            let note = box_
                .index
                .notes
                .iter_mut()
                .find(|n| n.id == note_id)
                .ok_or("花笺不存在")?;
            note.updated_at = now();
        }
        {
            let doc = box_.doc_mut(note_id)?;
            doc.body = content.to_string();
        }
        Self::repack(box_)
    }

    /// 复制当前正文为快照，永久保存到 md 文件中；返回快照元信息
    pub fn create_snapshot(
        &mut self,
        box_id: &str,
        note_id: &str,
        label: Option<String>,
    ) -> Result<SnapshotMeta, String> {
        let box_ = self.get_mut(box_id)?;
        {
            let note = box_
                .index
                .notes
                .iter_mut()
                .find(|n| n.id == note_id)
                .ok_or("花笺不存在")?;
            note.updated_at = now();
        }
        let meta;
        {
            let doc = box_.doc_mut(note_id)?;
            let version = doc.snapshots.len() as u32 + 1;
            meta = SnapshotMeta {
                id: Uuid::new_v4().to_string(),
                version,
                label: label.unwrap_or_else(|| format!("快照 {version}")),
                created_at: now(),
            };
            doc.snapshots.push(SnapshotDoc {
                meta: meta.clone(),
                content: doc.body.clone(),
            });
        }
        Self::repack(box_)?;
        Ok(meta)
    }

    /// 列出全部快照版本（来自目录段）
    pub fn list_snapshots(
        &self,
        box_id: &str,
        note_id: &str,
    ) -> Result<Vec<SnapshotMeta>, String> {
        let box_ = self.get(box_id)?;
        Ok(box_
            .doc(note_id)?
            .snapshots
            .iter()
            .map(|s| s.meta.clone())
            .collect())
    }

    /// 读取某个快照的内容
    pub fn read_snapshot(
        &self,
        box_id: &str,
        note_id: &str,
        snapshot_id: &str,
    ) -> Result<String, String> {
        let box_ = self.get(box_id)?;
        let doc = box_.doc(note_id)?;
        doc.snapshots
            .iter()
            .find(|s| s.meta.id == snapshot_id)
            .map(|s| s.content.clone())
            .ok_or("快照不存在".into())
    }

    /// 用快照内容替换正文（恢复版本），返回新正文
    pub fn apply_snapshot(
        &mut self,
        box_id: &str,
        note_id: &str,
        snapshot_id: &str,
    ) -> Result<String, String> {
        let box_ = self.get_mut(box_id)?;
        let content = {
            let doc = box_.doc(note_id)?;
            doc.snapshots
                .iter()
                .find(|s| s.meta.id == snapshot_id)
                .map(|s| s.content.clone())
                .ok_or("快照不存在")?
        };
        {
            let note = box_
                .index
                .notes
                .iter_mut()
                .find(|n| n.id == note_id)
                .ok_or("花笺不存在")?;
            note.updated_at = now();
        }
        {
            let doc = box_.doc_mut(note_id)?;
            doc.body = content.clone();
        }
        Self::repack(box_)?;
        Ok(content)
    }

    /// 删除某个快照版本
    pub fn delete_snapshot(
        &mut self,
        box_id: &str,
        note_id: &str,
        snapshot_id: &str,
    ) -> Result<(), String> {
        let box_ = self.get_mut(box_id)?;
        {
            let note = box_
                .index
                .notes
                .iter_mut()
                .find(|n| n.id == note_id)
                .ok_or("花笺不存在")?;
            note.updated_at = now();
        }
        {
            let doc = box_.doc_mut(note_id)?;
            let before = doc.snapshots.len();
            doc.snapshots.retain(|s| s.meta.id != snapshot_id);
            if doc.snapshots.len() == before {
                return Err("快照不存在".into());
            }
        }
        Self::repack(box_)
    }

    pub fn create_note(
        &mut self,
        box_id: &str,
        title: String,
        color: String,
    ) -> Result<NoteMeta, String> {
        let box_ = self.get_mut(box_id)?;
        let id = Uuid::new_v4().to_string();
        let file = format!("notes/{id}.md");
        let ts = now();
        let meta = NoteMeta {
            id: id.clone(),
            title,
            color,
            file: file.clone(),
            created_at: ts.clone(),
            updated_at: ts,
        };
        box_.index.notes.push(meta.clone());
        box_.docs.insert(file, NoteDoc::default());
        Self::save_index(box_)?;
        Ok(meta)
    }

    pub fn rename_note(
        &mut self,
        box_id: &str,
        note_id: &str,
        title: String,
    ) -> Result<(), String> {
        let box_ = self.get_mut(box_id)?;
        let updated = now();
        let note = box_
            .index
            .notes
            .iter_mut()
            .find(|n| n.id == note_id)
            .ok_or("花笺不存在")?;
        note.title = title;
        note.updated_at = updated;
        Self::save_index(box_)
    }

    pub fn delete_note(&mut self, box_id: &str, note_id: &str) -> Result<(), String> {
        let box_ = self.get_mut(box_id)?;
        let removed = box_.index.notes.iter().find(|n| n.id == note_id).cloned();
        if let Some(n) = removed {
            box_.index.notes.retain(|n| n.id != note_id);
            box_.docs.remove(&n.file);
        }
        Self::save_index(box_)
    }

    pub fn set_note_color(
        &mut self,
        box_id: &str,
        note_id: &str,
        color: String,
    ) -> Result<(), String> {
        let box_ = self.get_mut(box_id)?;
        let updated = now();
        let note = box_
            .index
            .notes
            .iter_mut()
            .find(|n| n.id == note_id)
            .ok_or("花笺不存在")?;
        note.color = color;
        note.updated_at = updated;
        Self::save_index(box_)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn open_tar_names(path: &Path) -> Vec<String> {
        let bytes = fs::read(path).unwrap();
        let mut archive = Archive::new(Cursor::new(bytes));
        let mut names = vec![];
        for e in archive.entries().unwrap() {
            let mut e = e.unwrap();
            let mut buf = String::new();
            e.read_to_string(&mut buf).unwrap();
            names.push(e.path().unwrap().to_string_lossy().into_owned());
            assert!(!buf.contains('\0'), "file should not be compressed/garbage");
        }
        names
    }

    #[test]
    fn roundtrip_create_open_edit() {
        let dir = std::env::temp_dir().join("huajian_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.hxl");

        // 创建空花匣
        let mut store = BoxStore::new();
        let info = store.create(&path, "测试花匣".into()).unwrap();
        assert_eq!(info.name, "测试花匣");
        assert_eq!(info.note_count, 0);

        // 新建花笺
        let note = store.create_note(&info.id, "第一篇".into(), "#b4753f".into()).unwrap();
        assert!(note.file.ends_with(".md"));

        // 写入内容
        store.write_note(&info.id, &note.id, "# 标题\n\n正文内容 **加粗**").unwrap();

        // 关闭并重新打开
        store.close(&info.id).unwrap();
        let info2 = store.open(&path).unwrap();
        assert_eq!(info2.note_count, 1);
        let md = store.read_note(&info2.id, &note.id).unwrap();
        assert!(md.contains("正文内容"));
        // 打开只返回正文，不包含分段标记
        assert!(!md.contains("hxl:"));

        // 重命名 + 改色
        store.rename_note(&info2.id, &note.id, "改名".into()).unwrap();
        store.set_note_color(&info2.id, &note.id, "#c0392b".into()).unwrap();
        let notes = store
            .get(&info2.id)
            .map(|b| b.index.notes.clone())
            .unwrap();
        assert_eq!(notes[0].title, "改名");
        assert_eq!(notes[0].color, "#c0392b");

        // 删除
        store.delete_note(&info2.id, &note.id).unwrap();
        let info3 = store.open(&path).unwrap();
        assert_eq!(info3.note_count, 0);

        // 用 tar 校验文件结构
        assert_eq!(open_tar_names(&path), vec!["box.json"]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshot_roundtrip() {
        let dir = std::env::temp_dir().join("huajian_snapshot_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("snap.hxl");

        let mut store = BoxStore::new();
        let info = store.create(&path, "快照测试".into()).unwrap();
        let note = store.create_note(&info.id, "笔记".into(), "#b4753f".into()).unwrap();

        store.write_note(&info.id, &note.id, "第一版").unwrap();
        let s1 = store.create_snapshot(&info.id, &note.id, None).unwrap();
        assert_eq!(s1.version, 1);
        assert_eq!(s1.label, "快照 1");

        store.write_note(&info.id, &note.id, "第二版").unwrap();
        let s2 = store.create_snapshot(&info.id, &note.id, Some("初稿".into())).unwrap();
        assert_eq!(s2.version, 2);

        // 列出 + 读取
        let list = store.list_snapshots(&info.id, &note.id).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(store.read_snapshot(&info.id, &note.id, &s1.id).unwrap(), "第一版");
        assert_eq!(store.read_snapshot(&info.id, &note.id, &s2.id).unwrap(), "第二版");

        // 恢复 v1 到正文
        let body = store.apply_snapshot(&info.id, &note.id, &s1.id).unwrap();
        assert_eq!(body, "第一版");
        assert_eq!(store.read_note(&info.id, &note.id).unwrap(), "第一版");

        // 删除 v2
        store.delete_snapshot(&info.id, &note.id, &s2.id).unwrap();
        let list = store.list_snapshots(&info.id, &note.id).unwrap();
        assert_eq!(list.len(), 1);

        // 重开校验持久化
        store.close(&info.id).unwrap();
        let info2 = store.open(&path).unwrap();
        assert_eq!(store.read_note(&info2.id, &note.id).unwrap(), "第一版");
        let list = store.list_snapshots(&info2.id, &note.id).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].version, 1);
        assert_eq!(store.read_snapshot(&info2.id, &note.id, &list[0].id).unwrap(), "第一版");

        // tar 内部结构：box.json + notes/*.md
        let names = open_tar_names(&path);
        assert_eq!(names.len(), 2);
        assert!(names[0] == "box.json");
        assert!(names[1].starts_with("notes/") && names[1].ends_with(".md"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_note_without_markers_is_all_body() {
        let dir = std::env::temp_dir().join("huajian_legacy_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("legacy.hxl");

        // 手工构造一个旧版 hxl：notes 直接是纯 markdown
        let mut store = BoxStore::new();
        let info = store.create(&path, "旧版".into()).unwrap();
        let note = store.create_note(&info.id, "旧笔记".into(), "#b4753f".into()).unwrap();
        store.close(&info.id).unwrap();
        let info = store.open(&path).unwrap();
        store.write_note(&info.id, &note.id, "旧正文").unwrap();
        // 用旧格式（无标记）重写 md 内容：直接篡改 tar 模拟旧版文件
        {
            let raw = fs::read(&path).unwrap();
            let mut archive = Archive::new(Cursor::new(raw));
            let mut entries_map: HashMap<String, Vec<u8>> = HashMap::new();
            for e in archive.entries().unwrap() {
                let mut e = e.unwrap();
                let name = e.path().unwrap().to_string_lossy().into_owned();
                let mut buf = Vec::new();
                e.read_to_end(&mut buf).unwrap();
                entries_map.insert(name, buf);
            }
            entries_map.insert(note.file.clone(), "纯正文无标记".into());
            let tmp = path.with_extension("hxl.tmp");
            let file = fs::File::create(&tmp).unwrap();
            let mut builder = Builder::new(file);
            let mut names: Vec<_> = entries_map.keys().cloned().collect();
            names.sort();
            for name in names {
                let mut header = Header::new_gnu();
                header.set_size(entries_map[&name].len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder
                    .append_data(&mut header, &name, entries_map[&name].as_slice())
                    .unwrap();
            }
            builder.finish().unwrap();
            fs::rename(&tmp, &path).unwrap();
        }

        // 重新打开：旧文件整体视为正文
        store.close(&info.id).unwrap();
        let info2 = store.open(&path).unwrap();
        assert_eq!(store.read_note(&info2.id, &note.id).unwrap(), "纯正文无标记");
        let list = store.list_snapshots(&info2.id, &note.id).unwrap();
        assert!(list.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }
}
