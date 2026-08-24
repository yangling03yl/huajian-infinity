use std::collections::HashMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use chrono::Utc;
use tar::{Archive, Builder, Header};
use uuid::Uuid;

use crate::models::{BoxIndex, BoxInfo, NoteMeta};

const INDEX_FILE: &str = "box.json";

pub struct OpenBox {
    pub id: String,
    pub path: PathBuf,
    pub index: BoxIndex,
    pub files: HashMap<String, Vec<u8>>,
}

impl OpenBox {
    pub fn note(&self, note_id: &str) -> Option<&NoteMeta> {
        self.index.notes.iter().find(|n| n.id == note_id)
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

    /// 打开一个 .hxl 花匣：解析 tar、读取 box.json 索引、文件全部载入内存
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

        for note in &index.notes {
            files.entry(note.file.clone()).or_default();
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
            OpenBox { id, path: path.to_path_buf(), index, files },
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
        };
        Self::repack(&ob)?;
        let info = BoxInfo {
            id: id.clone(),
            path: path.display().to_string(),
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

        let mut names: Vec<&String> = bx.files.keys().collect();
        names.sort();
        for name in names {
            let mut header = Header::new_gnu();
            header.set_size(bx.files[name].len() as u64);
            header.set_mode(0o644);
            header.set_mtime(now_epoch());
            header.set_cksum();
            builder
                .append_data(&mut header, name, bx.files[name].as_slice())
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

    pub fn read_note(&self, box_id: &str, note_id: &str) -> Result<String, String> {
        let box_ = self.get(box_id)?;
        let note = box_.note(note_id).ok_or("花笺不存在")?;
        let content = box_
            .files
            .get(&note.file)
            .ok_or("花笺文件缺失")?;
        Ok(String::from_utf8_lossy(content).into_owned())
    }

    pub fn write_note(
        &mut self,
        box_id: &str,
        note_id: &str,
        content: &str,
    ) -> Result<(), String> {
        let box_ = self.get_mut(box_id)?;
        let file;
        {
            let note = box_
                .index
                .notes
                .iter_mut()
                .find(|n| n.id == note_id)
                .ok_or("花笺不存在")?;
            file = note.file.clone();
            note.updated_at = now();
        }
        box_.files.insert(file, content.as_bytes().to_vec());
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
            file,
            created_at: ts.clone(),
            updated_at: ts,
        };
        box_.index.notes.push(meta.clone());
        box_.files.insert(meta.file.clone(), Vec::new());
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
            box_.files.remove(&n.file);
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
        let bytes = fs::read(&path).unwrap();
        let mut archive = Archive::new(Cursor::new(bytes));
        let mut names = vec![];
        for e in archive.entries().unwrap() {
            let mut e = e.unwrap();
            let mut buf = String::new();
            e.read_to_string(&mut buf).unwrap();
            names.push(e.path().unwrap().to_string_lossy().into_owned());
            assert!(!buf.contains('\0'), "file should not be compressed/garbage");
        }
        assert_eq!(names, vec!["box.json"]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn debug_open_python_tar() {
        let path = std::path::Path::new("/tmp/opencode/testbox/示例.hxl");
        let mut store = BoxStore::new();
        match store.open(path) {
            Ok(info) => println!("OPEN-OK: {} notes={}", info.name, info.note_count),
            Err(e) => println!("OPEN-ERR: {}", e),
        }
    }
}
