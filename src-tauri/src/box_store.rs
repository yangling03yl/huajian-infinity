use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use chrono::Utc;
use tar::{Archive, Builder, Header};
use uuid::Uuid;

use crate::models::{BoxIndex, BoxInfo, NoteMeta, SnapshotMeta};
use crate::note_doc::{self, NoteDoc, SnapshotDoc};

const INDEX_FILE: &str = "box.json";

/// 花匣名的唯一来源：`.hxl` 文件的「无后缀文件名」。
///
/// 返回 `None` 表示路径没有可用的文件名（`/`、`.hxl`、`..` 等）。
/// 刻意不做 trim：名字必须与磁盘文件名严格一致，否则「文件名 = 名字」这条不变量就破了。
pub fn box_name_from_path(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_string_lossy().into_owned();
    // 纯隐藏文件（`.hxl`、`.花匣`）：Rust 的 `file_stem` 会把 `.hxl` 原样返回，
    // 照旧使用就会造出一个「没有名字、还看不见」的花匣
    if file_name.starts_with('.') && !file_name[1..].contains('.') {
        return None;
    }
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    if stem.trim().is_empty() || stem == "." || stem == ".." {
        return None;
    }
    Some(stem)
}

/// 派生名字的兜底版本，供展示与日志使用
pub fn box_name_or_default(path: &Path) -> String {
    box_name_from_path(path).unwrap_or_else(|| "未命名花匣".to_string())
}

/// 路径比较用的归一化键：只统一分隔符，**不改变大小写**
fn norm_key(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// 平台默认文件系统是否大小写不敏感（Windows / macOS）。
///
/// 只在 canonicalize 失败（目标尚不存在，例如改名目标）时作为兜底；
/// Linux 上恒为 false —— 否则 `A.hxl` 与 `a.hxl` 这两个**真实存在的不同文件**
/// 会被判成同一个花匣，用户编辑 A 却写进 a。
fn case_insensitive_fallback() -> bool {
    cfg!(any(windows, target_os = "macos"))
}

/// 两个路径是否指向同一花匣文件。
///
/// 优先 canonicalize（处理符号链接/相对路径/`..`）并做**精确**比较；
/// Linux 上到此为止 —— `A.hxl` 与 `a.hxl` 是两个真实文件，不能合并。
///
/// 仅在大小写不敏感的平台（Windows/macOS）上才追加一次忽略大小写的比较：
/// 这些平台 `canonicalize` 一般已经返回磁盘上的真实大小写，但极端情况下
/// （verbatim 路径、网络盘）仍可能给出只差大小写的两个字符串，此时不能判成两个花匣。
fn same_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    if let (Ok(ca), Ok(cb)) = (fs::canonicalize(a), fs::canonicalize(b)) {
        if ca == cb {
            return true;
        }
        return case_insensitive_fallback()
            && norm_key(&ca).to_lowercase() == norm_key(&cb).to_lowercase();
    }
    let (ka, kb) = (norm_key(a), norm_key(b));
    if ka == kb {
        return true;
    }
    case_insensitive_fallback() && ka.to_lowercase() == kb.to_lowercase()
}

/// 花匣名最大字符数（远小于文件系统单段 255 字节上限，给 `.hxl` 与目录留余量）
const MAX_BOX_NAME_CHARS: usize = 120;

/// Windows 保留设备名（不分大小写；带扩展名同样非法，故按第一个 `.` 之前判断）
const WINDOWS_RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// 校验用户输入的花匣名，返回可直接展示给用户的中文错误。
///
/// 采用「拒绝」而不是「自动剔除非法字符」：静默改写会让用户看到的输入与实际文件名不一致。
/// 非法字符集与前端 `export.ts::safeFileName` 保持一致。
pub fn sanitize_box_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("花匣名不能为空".into());
    }
    if name.chars().count() > MAX_BOX_NAME_CHARS {
        return Err(format!("花匣名过长（最多 {MAX_BOX_NAME_CHARS} 个字符）"));
    }
    if name.chars().any(|c| c.is_control()) {
        return Err("花匣名不能包含控制字符".into());
    }
    if let Some(c) = name
        .chars()
        .find(|c| matches!(c, '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*'))
    {
        return Err(format!(
            "花匣名不能包含字符「{c}」（也不能包含 / \\ < > : \" | ? *）"
        ));
    }
    if name == "." || name == ".." {
        return Err("花匣名不合法".into());
    }
    if name.ends_with('.') {
        return Err("花匣名不能以「.」结尾（Windows 会静默去掉）".into());
    }
    let lower = name.to_lowercase();
    if lower.ends_with(".hxl") {
        return Err("花匣名不要以「.hxl」结尾（扩展名会自动补上）".into());
    }
    let stem = lower.split('.').next().unwrap_or("");
    if WINDOWS_RESERVED.contains(&stem) {
        return Err(format!("「{name}」是 Windows 保留名，请换一个"));
    }
    Ok(name.to_string())
}

/// 把重命名的 IO 错误翻成用户能看懂的话（Windows 上「被占用」最常见）
fn rename_err_msg(name: &str, e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => format!(
            "重命名为「{name}」失败：文件可能正被其它程序占用（同步盘或杀毒软件），请关闭后重试。（{e}）"
        ),
        std::io::ErrorKind::AlreadyExists => {
            format!("重命名为「{name}」失败：目标文件已存在。（{e}）")
        }
        _ => format!("重命名为「{name}」失败: {e}"),
    }
}

pub struct OpenBox {
    pub id: String,
    pub path: PathBuf,
    pub index: BoxIndex,
    /// 花匣内的其他文件（box.json 等）
    pub files: HashMap<String, Vec<u8>>,
    /// 各花笺的结构化文档（正文 + 快照），键为 note.file
    pub docs: HashMap<String, NoteDoc>,
    /// 本会话是否已写过一次 `.hxl.bak`（每次保存都复制整包代价太大，故只做一次）
    pub backup_done: bool,
}

/// 判断索引里的 `note.file` 是否可以直接使用（相对、无 `..`、不占用 box.json）
fn is_safe_note_file(file: &str) -> bool {
    if file.is_empty() || file == INDEX_FILE {
        return false;
    }
    if file.starts_with('/') || file.starts_with('\\') || file.contains('\\') {
        return false;
    }
    if file.chars().any(|c| c.is_control()) {
        return false;
    }
    file.split('/')
        .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}

/// 规范化（必要时重写）`note.file`。
///
/// `box.json` 是外部可控数据：一个被构造过的花匣可以把 `file` 指向 `box.json`
/// 本身或 `../../x`，一旦保存就会写出重复条目把索引顶掉，花匣自此再也打不开。
/// 这里把不安全或重复的路径统一改写成 `notes/<id>.md`，内容若有则跟着搬过去，
/// 只在内存里修正，下一次保存自动落盘（与花匣名的回填策略一致）。
fn sanitize_note_file(file: &str, note_id: &str, used: &HashSet<String>) -> String {
    let id_part: String = note_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let base = if id_part.is_empty() { Uuid::new_v4().to_string() } else { id_part };

    if is_safe_note_file(file) && !used.contains(file) {
        return file.to_string();
    }
    let mut candidate = format!("notes/{base}.md");
    let mut n = 2;
    while used.contains(&candidate) {
        candidate = format!("notes/{base}-{n}.md");
        n += 1;
    }
    candidate
}

impl OpenBox {
    pub fn note(&self, note_id: &str) -> Option<&NoteMeta> {
        self.index.notes.iter().find(|n| n.id == note_id)
    }

    fn doc(&self, note_id: &str) -> Result<&NoteDoc, String> {
        let note = self.note(note_id).ok_or("花笺不存在")?;
        self.docs.get(&note.file).ok_or("花笺文件缺失".into())
    }
}

pub struct BoxStore {
    pub boxes: HashMap<String, OpenBox>,
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn now_epoch() -> u64 {
    let ts = Utc::now().timestamp();
    // 系统时钟早于 1970 时 timestamp() 为负，直接 as u64 会变成天文数字
    if ts > 0 {
        ts as u64
    } else {
        0
    }
}

/// 目录项也落盘，保证 `rename` 本身在掉电后可见（Windows 不支持打开目录，跳过）
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    {
        if let Ok(d) = fs::File::open(dir) {
            let _ = d.sync_all();
        }
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
    }
}

impl BoxStore {
    pub fn new() -> Self {
        Self { boxes: HashMap::new() }
    }

    /// 打开一个 .hxl 花匣：解析 tar、读取 box.json 索引、笔记解析为正文+快照。
    ///
    /// 花匣名以「文件名」为准：box.json 里的 name 只作为派生缓存，打开时被文件名覆盖
    /// （只在内存纠正，磁盘留到下一次任何保存时由 repack 回填，避免读操作变成写操作）。
    pub fn open(&mut self, path: &Path) -> Result<BoxInfo, String> {
        // 同一文件不重复打开：直接复用已有条目，否则同一路径会出现两个 id，
        // 连带 settings.boxes 重复、侧栏两个条目
        if let Some(b) = self.boxes.values().find(|b| same_path(&b.path, path)) {
            let id = b.id.clone();
            return self.info_of(&id);
        }

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
        let mut index: BoxIndex =
            serde_json::from_slice(raw).map_err(|e| format!("box.json 解析失败: {e}"))?;

        // 名字以文件名为准，box.json 的 name 不参与展示
        index.name = box_name_or_default(path);

        // note.file 是外部可控字段：先规范化，避免 `box.json`/`..`/绝对路径把花匣写坏
        let mut used: HashSet<String> = HashSet::new();
        used.insert(INDEX_FILE.to_string());
        let mut docs: HashMap<String, NoteDoc> = HashMap::new();
        for note in &mut index.notes {
            let safe = sanitize_note_file(&note.file, &note.id, &used);
            if safe != note.file {
                if let Some(bytes) = files.remove(&note.file) {
                    files.insert(safe.clone(), bytes);
                }
                note.file = safe.clone();
            }
            used.insert(safe.clone());
            let doc = files
                .remove(&safe)
                .map(|bytes| note_doc::parse(&String::from_utf8_lossy(&bytes)))
                .unwrap_or_default();
            docs.insert(safe, doc);
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
            OpenBox {
                id,
                path: path.to_path_buf(),
                index,
                files,
                docs,
                backup_done: false,
            },
        );
        Ok(info)
    }

    /// 新建一个空花匣并写入磁盘。名字只来自文件名，不接收外部传名。
    pub fn create(&mut self, path: &Path) -> Result<BoxInfo, String> {
        if path.exists() {
            return Err("目标文件已存在".into());
        }
        // 同一路径已经打开时必须拒绝：否则会出现两个 OpenBox 指向同一个文件，
        // 各自 repack 时互相整体覆盖（open 侧有去重，create 侧必须有对应的检查）
        if self.boxes.values().any(|b| same_path(&b.path, path)) {
            return Err("该文件已打开".into());
        }
        // 没有可用文件名（如 `.hxl`）直接拒绝，避免造出无名花匣
        let name = box_name_from_path(path).ok_or("文件名不能为空")?;
        let id = Uuid::new_v4().to_string();
        let index = BoxIndex {
            version: 1,
            name,
            created_at: now(),
            notes: Vec::new(),
        };
        let mut ob = OpenBox {
            id: id.clone(),
            path: path.to_path_buf(),
            index,
            files: HashMap::new(),
            docs: HashMap::new(),
            // 新建时磁盘上还没有文件，无需备份
            backup_done: true,
        };
        Self::repack(&mut ob)?;
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

    /// 由内存状态组装当前 BoxInfo（open 去重与 rename 复用）
    fn info_of(&self, box_id: &str) -> Result<BoxInfo, String> {
        let b = self.get(box_id)?;
        Ok(BoxInfo {
            id: b.id.clone(),
            path: b.path.display().to_string(),
            name: b.index.name.clone(),
            note_count: b.index.notes.len(),
        })
    }

    /// 重命名花匣 = 真正重命名磁盘上的 `.hxl` 文件，并把新名字写回 box.json。
    ///
    /// 顺序上所有可能失败的 I/O 都在 `fs::rename` 之前完成，改名成功之后不再写盘，
    /// 因此不存在「文件已改名但内存/设置没跟上」的中间态。返回更新后的 BoxInfo（id 不变）。
    pub fn rename_box(&mut self, box_id: &str, new_name: &str) -> Result<BoxInfo, String> {
        let name = sanitize_box_name(new_name)?;

        let (old_path, old_name) = {
            let b = self.get(box_id)?;
            (b.path.clone(), b.index.name.clone())
        };
        let parent = old_path.parent().ok_or("花匣路径不合法")?.to_path_buf();
        let new_path = parent.join(format!("{name}.hxl"));

        // 名字与文件名已经一致：不动磁盘
        if new_path == old_path {
            return self.info_of(box_id);
        }

        let new_file = new_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        // 另一个已打开的花匣占用了该路径
        if let Some(other) = self
            .boxes
            .values()
            .find(|b| b.id != box_id && same_path(&b.path, &new_path))
        {
            return Err(format!(
                "「{}」已打开，不能使用同一个文件",
                box_name_or_default(&other.path)
            ));
        }

        // 目标已存在时只允许一种情况：它就是当前这个文件本身（大小写不敏感文件系统上
        // 仅改大小写会命中旧文件）。其余一律拒绝 —— 绝不能让 fs::rename 覆盖掉
        // 同目录下另一个花匣（Linux 上 `Book.hxl` 与 `book.hxl` 是两个真实文件）。
        let target_exists = fs::symlink_metadata(&new_path).is_ok();
        if target_exists && !same_path(&old_path, &new_path) {
            return Err(format!("同目录下已存在「{new_file}」"));
        }

        // 先把内存内容（含新名字）落盘到旧路径：等价一次强制保存
        {
            let b = self.get_mut(box_id)?;
            b.index.name = name.clone();
        }
        let repack_err = {
            let b = self.get_mut(box_id)?;
            Self::repack(b).err()
        };
        if let Some(e) = repack_err {
            if let Ok(b) = self.get_mut(box_id) {
                b.index.name = old_name.clone();
            }
            return Err(e);
        }

        // 真正改名。目标存在（同一文件、仅大小写不同）时走两步，
        // 临时名带 uuid 且以 `.` 开头，既避开 repack 的临时文件，也不会被当成 .hxl 打开
        let renamed = if target_exists {
            let mid = parent.join(format!(".hxl-rename-{}.tmp", Uuid::new_v4()));
            match fs::rename(&old_path, &mid) {
                Ok(()) => match fs::rename(&mid, &new_path) {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        let _ = fs::rename(&mid, &old_path);
                        Err(e)
                    }
                },
                Err(e) => Err(e),
            }
        } else {
            fs::rename(&old_path, &new_path)
        };

        if let Err(e) = renamed {
            // 磁盘仍是旧文件：内存名字还原，并尽力把 box.json 写回旧名字
            if let Ok(b) = self.get_mut(box_id) {
                b.index.name = old_name;
                let _ = Self::repack(b);
            }
            return Err(rename_err_msg(&name, &e));
        }

        // 成功后只改内存路径（新名字在上一步已写进 box.json）
        {
            let b = self.get_mut(box_id)?;
            b.path = new_path;
        }
        self.info_of(box_id)
    }

    /// 将花匣全量重打包为 tar（无压缩），临时文件 + 原子重命名。
    ///
    /// `box.json` 一律由内存 `index` 重新序列化，保证「文件名 / 内存 name / 磁盘 name」三处
    /// 永远一致，也让 open 时的名字回填在任何一次保存后自动生效。
    ///
    /// 加固点：
    /// - 临时文件名带 uuid 且以 `.` 开头，不会撞上同目录下别的花匣
    ///   （旧实现用 `<stem>.hxl.tmp`，恰好能覆盖掉一个真实存在的 `a.hxl.tmp`）；
    /// - `finish` 之后 `sync_all` 再 `rename`，掉电不会拿到半截文件；
    /// - 本会话第一次保存时把原文件复制为 `<name>.hxl.bak`，给整包损坏留退路；
    /// - 条目用 `Cow` 借用写入，不再把整个花匣逐条 clone 一遍。
    pub fn repack(bx: &mut OpenBox) -> Result<(), String> {
        let dir = bx.path.parent().ok_or("花匣路径不合法")?.to_path_buf();
        let tmp = dir.join(format!(".hxl-tmp-{}.tmp", Uuid::new_v4()));

        if let Err(e) = Self::pack_to(bx, &tmp) {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }

        // 覆盖前留一份备份（每个打开的花匣只做一次，避免每次自动保存都复制整包）
        if !bx.backup_done && bx.path.exists() {
            let bak = bx.path.with_extension("hxl.bak");
            let _ = fs::copy(&bx.path, &bak);
        }

        if let Err(e) = fs::rename(&tmp, &bx.path) {
            let _ = fs::remove_file(&tmp);
            return Err(format!("保存花匣失败: {e}"));
        }
        sync_dir(&dir);
        bx.backup_done = true;
        Ok(())
    }

    /// 真正写 tar 到 `tmp`（不负责改名/备份，失败时由调用方清理）
    fn pack_to(bx: &OpenBox, tmp: &Path) -> Result<(), String> {
        let index_json = serde_json::to_vec_pretty(&bx.index).map_err(|e| e.to_string())?;
        let docs: Vec<(String, String)> = bx
            .docs
            .iter()
            .map(|(file, doc)| (file.clone(), note_doc::serialize(doc)))
            .collect();

        let mut entries: Vec<(&str, Cow<'_, [u8]>)> =
            Vec::with_capacity(bx.files.len() + docs.len() + 1);
        for (name, bytes) in &bx.files {
            if name.as_str() == INDEX_FILE {
                continue;
            }
            entries.push((name.as_str(), Cow::Borrowed(bytes.as_slice())));
        }
        for (name, text) in &docs {
            entries.push((name.as_str(), Cow::Borrowed(text.as_bytes())));
        }
        entries.push((INDEX_FILE, Cow::Owned(index_json)));
        entries.sort_by(|a, b| a.0.cmp(b.0));

        let file = fs::File::create(tmp).map_err(|e| format!("创建临时文件失败: {e}"))?;
        let mut builder = Builder::new(file);
        builder.mode(tar::HeaderMode::Deterministic);
        for (name, bytes) in entries {
            let mut header = Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_mtime(now_epoch());
            header.set_cksum();
            builder
                .append_data(&mut header, name, bytes.as_ref())
                .map_err(|e| format!("写入花匣失败: {e}"))?;
        }
        builder.finish().map_err(|e| format!("打包花匣失败: {e}"))?;
        let file = builder.into_inner().map_err(|e| format!("打包花匣失败: {e}"))?;
        file.sync_all().map_err(|e| format!("写入花匣失败: {e}"))?;
        Ok(())
    }

    /// 花笺元信息变更后落盘：box.json 由 repack 统一写入，这里只需重打包
    pub fn save_index(bx: &mut OpenBox) -> Result<(), String> {
        Self::repack(bx)
    }

    /// 读取正文（不返回快照与目录段）
    pub fn read_note(&self, box_id: &str, note_id: &str) -> Result<String, String> {
        let box_ = self.get(box_id)?;
        Ok(box_.doc(note_id)?.body.clone())
    }

    /// 写入正文（保留快照与目录段）
    ///
    /// 内容与内存中一致时直接返回：切换标签/重复 flush 不再触发整包重写。
    /// 写盘失败时把正文与 `updated_at` 一起回滚，避免内存与磁盘长期不一致。
    pub fn write_note(
        &mut self,
        box_id: &str,
        note_id: &str,
        content: &str,
    ) -> Result<(), String> {
        let box_ = self.get_mut(box_id)?;
        let file = box_.note(note_id).ok_or("花笺不存在")?.file.clone();
        if box_.docs.get(&file).map(|d| d.body.as_str()) == Some(content) {
            return Ok(());
        }
        let idx = box_
            .index
            .notes
            .iter()
            .position(|n| n.id == note_id)
            .ok_or("花笺不存在")?;
        let old_body = box_.docs.get(&file).map(|d| d.body.clone()).unwrap_or_default();
        let old_updated = box_.index.notes[idx].updated_at.clone();

        box_.index.notes[idx].updated_at = now();
        if let Some(doc) = box_.docs.get_mut(&file) {
            doc.body = content.to_string();
        }

        if let Err(e) = Self::repack(box_) {
            if let Some(doc) = box_.docs.get_mut(&file) {
                doc.body = old_body;
            }
            box_.index.notes[idx].updated_at = old_updated;
            return Err(e);
        }
        Ok(())
    }

    /// 复制当前正文为快照，永久保存到 md 文件中；返回快照元信息
    pub fn create_snapshot(
        &mut self,
        box_id: &str,
        note_id: &str,
        label: Option<String>,
    ) -> Result<SnapshotMeta, String> {
        let box_ = self.get_mut(box_id)?;
        let file = box_.note(note_id).ok_or("花笺不存在")?.file.clone();
        let idx = box_
            .index
            .notes
            .iter()
            .position(|n| n.id == note_id)
            .ok_or("花笺不存在")?;
        let old_updated = box_.index.notes[idx].updated_at.clone();

        // 版本号取「现有最大版本 + 1」：删掉中间某个快照后再新建，不会与现存版本撞号
        let version = box_
            .docs
            .get(&file)
            .and_then(|d| d.snapshots.iter().map(|s| s.meta.version).max())
            .unwrap_or(0)
            + 1;
        let meta = SnapshotMeta {
            id: Uuid::new_v4().to_string(),
            version,
            label: label.unwrap_or_else(|| format!("快照 {version}")),
            created_at: now(),
        };
        let body = box_.docs.get(&file).map(|d| d.body.clone()).unwrap_or_default();

        box_.index.notes[idx].updated_at = now();
        if let Some(doc) = box_.docs.get_mut(&file) {
            doc.snapshots.push(SnapshotDoc { meta: meta.clone(), content: body });
        }

        if let Err(e) = Self::repack(box_) {
            if let Some(doc) = box_.docs.get_mut(&file) {
                doc.snapshots.pop();
            }
            box_.index.notes[idx].updated_at = old_updated;
            return Err(e);
        }
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
        let file = box_.note(note_id).ok_or("花笺不存在")?.file.clone();
        let content = box_
            .docs
            .get(&file)
            .and_then(|doc| doc.snapshots.iter().find(|s| s.meta.id == snapshot_id))
            .map(|s| s.content.clone())
            .ok_or("快照不存在")?;
        let idx = box_
            .index
            .notes
            .iter()
            .position(|n| n.id == note_id)
            .ok_or("花笺不存在")?;
        let old_body = box_.docs.get(&file).map(|d| d.body.clone()).unwrap_or_default();
        let old_updated = box_.index.notes[idx].updated_at.clone();

        box_.index.notes[idx].updated_at = now();
        if let Some(doc) = box_.docs.get_mut(&file) {
            doc.body = content.clone();
        }

        if let Err(e) = Self::repack(box_) {
            if let Some(doc) = box_.docs.get_mut(&file) {
                doc.body = old_body;
            }
            box_.index.notes[idx].updated_at = old_updated;
            return Err(e);
        }
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
        let file = box_.note(note_id).ok_or("花笺不存在")?.file.clone();
        // 先校验存在性：校验通过之前不触碰任何状态，失败时不会留下「改了时间戳但没删掉」
        let pos = box_
            .docs
            .get(&file)
            .and_then(|doc| doc.snapshots.iter().position(|s| s.meta.id == snapshot_id))
            .ok_or("快照不存在")?;
        let idx = box_
            .index
            .notes
            .iter()
            .position(|n| n.id == note_id)
            .ok_or("花笺不存在")?;
        let old_updated = box_.index.notes[idx].updated_at.clone();
        let removed = box_
            .docs
            .get_mut(&file)
            .map(|doc| doc.snapshots.remove(pos));
        box_.index.notes[idx].updated_at = now();

        if let Err(e) = Self::repack(box_) {
            if let (Some(snap), Some(doc)) = (removed, box_.docs.get_mut(&file)) {
                doc.snapshots.insert(pos, snap);
            }
            box_.index.notes[idx].updated_at = old_updated;
            return Err(e);
        }
        Ok(())
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
        box_.docs.insert(file.clone(), NoteDoc::default());
        if let Err(e) = Self::save_index(box_) {
            // 写盘失败：把刚插入的花笺撤掉，避免内存里留一个「盘上没有」的幽灵花笺，
            // 下一次任意保存又把它写进 box.json
            box_.index.notes.retain(|n| n.id != meta.id);
            box_.docs.remove(&file);
            return Err(e);
        }
        Ok(meta)
    }

    pub fn rename_note(
        &mut self,
        box_id: &str,
        note_id: &str,
        title: String,
    ) -> Result<(), String> {
        let box_ = self.get_mut(box_id)?;
        let idx = box_
            .index
            .notes
            .iter()
            .position(|n| n.id == note_id)
            .ok_or("花笺不存在")?;
        let old_title = std::mem::replace(&mut box_.index.notes[idx].title, title);
        let old_updated = std::mem::replace(&mut box_.index.notes[idx].updated_at, now());
        if let Err(e) = Self::save_index(box_) {
            box_.index.notes[idx].title = old_title;
            box_.index.notes[idx].updated_at = old_updated;
            return Err(e);
        }
        Ok(())
    }

    pub fn delete_note(&mut self, box_id: &str, note_id: &str) -> Result<(), String> {
        let box_ = self.get_mut(box_id)?;
        // 删除不存在的花笺要报错：静默成功会让前端以为删掉了
        let pos = box_
            .index
            .notes
            .iter()
            .position(|n| n.id == note_id)
            .ok_or("花笺不存在")?;
        let removed = box_.index.notes.remove(pos);
        let file = removed.file.clone();
        let doc = box_.docs.remove(&file);
        if let Err(e) = Self::save_index(box_) {
            box_.index.notes.insert(pos, removed);
            if let Some(d) = doc {
                box_.docs.insert(file, d);
            }
            return Err(e);
        }
        Ok(())
    }

    pub fn set_note_color(
        &mut self,
        box_id: &str,
        note_id: &str,
        color: String,
    ) -> Result<(), String> {
        let box_ = self.get_mut(box_id)?;
        let idx = box_
            .index
            .notes
            .iter()
            .position(|n| n.id == note_id)
            .ok_or("花笺不存在")?;
        let old_color = std::mem::replace(&mut box_.index.notes[idx].color, color);
        let old_updated = std::mem::replace(&mut box_.index.notes[idx].updated_at, now());
        if let Err(e) = Self::save_index(box_) {
            box_.index.notes[idx].color = old_color;
            box_.index.notes[idx].updated_at = old_updated;
            return Err(e);
        }
        Ok(())
    }

    /// 按新顺序保存花匣内花笺列表（order 为花笺 id 的有序列表）。
    /// order 未覆盖的花笺保持原有相对顺序排在末尾，不会丢失；不改变 updated_at。
    pub fn reorder_notes(&mut self, box_id: &str, order: Vec<String>) -> Result<(), String> {
        let box_ = self.get_mut(box_id)?;
        let before = box_.index.notes.clone();
        let rank: HashMap<&str, usize> = order
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();
        // 稳定排序：order 之外的花笺按原相对顺序落在末尾
        box_.index
            .notes
            .sort_by_key(|n| rank.get(n.id.as_str()).copied().unwrap_or(order.len()));
        if let Err(e) = Self::save_index(box_) {
            box_.index.notes = before;
            return Err(e);
        }
        Ok(())
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

    /// 读出 tar 内指定条目的原始字节
    fn read_entry(path: &Path, want: &str) -> Vec<u8> {
        let bytes = fs::read(path).unwrap();
        let mut archive = Archive::new(Cursor::new(bytes));
        for e in archive.entries().unwrap() {
            let mut e = e.unwrap();
            if e.path().unwrap().to_string_lossy() == want {
                let mut buf = Vec::new();
                e.read_to_end(&mut buf).unwrap();
                return buf;
            }
        }
        panic!("tar 内缺少 {want}");
    }

    /// 读出 tar 内 box.json 的 name 字段
    fn read_index_name(path: &Path) -> String {
        let index: BoxIndex = serde_json::from_slice(&read_entry(path, INDEX_FILE)).unwrap();
        index.name
    }

    /// 用给定条目重写整个 tar（沿用 legacy 测试的写法）
    fn write_tar(path: &Path, entries: &HashMap<String, Vec<u8>>) {
        let tmp = path.with_extension("hxl.tmp");
        let file = fs::File::create(&tmp).unwrap();
        let mut builder = Builder::new(file);
        builder.mode(tar::HeaderMode::Deterministic);
        let mut names: Vec<&String> = entries.keys().collect();
        names.sort();
        for name in names {
            let mut header = Header::new_gnu();
            header.set_size(entries[name].len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, name.as_str(), entries[name].as_slice())
                .unwrap();
        }
        builder.finish().unwrap();
        fs::rename(&tmp, path).unwrap();
    }

    /// 把 box.json 的 name 篡改成其它值，模拟旧文件或外部改写
    fn tamper_index_name(path: &Path, new_name: &str) {
        let bytes = fs::read(path).unwrap();
        let mut archive = Archive::new(Cursor::new(bytes));
        let mut entries: HashMap<String, Vec<u8>> = HashMap::new();
        for e in archive.entries().unwrap() {
            let mut e = e.unwrap();
            let name = e.path().unwrap().to_string_lossy().into_owned();
            let mut buf = Vec::new();
            e.read_to_end(&mut buf).unwrap();
            entries.insert(name, buf);
        }
        let mut index: BoxIndex = serde_json::from_slice(&entries[INDEX_FILE]).unwrap();
        index.name = new_name.to_string();
        entries.insert(
            INDEX_FILE.to_string(),
            serde_json::to_vec_pretty(&index).unwrap(),
        );
        write_tar(path, &entries);
    }

    #[test]
    fn roundtrip_create_open_edit() {
        let dir = std::env::temp_dir().join("huajian_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.hxl");

        // 创建空花匣：名字取文件名
        let mut store = BoxStore::new();
        let info = store.create(&path).unwrap();
        assert_eq!(info.name, "test");
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
        let info = store.create(&path).unwrap();
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
    fn reorder_notes_persists_order() {
        let dir = std::env::temp_dir().join("huajian_reorder_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("reorder.hxl");

        let mut store = BoxStore::new();
        let info = store.create(&path).unwrap();
        let n1 = store.create_note(&info.id, "一".into(), "#b4753f".into()).unwrap();
        let n2 = store.create_note(&info.id, "二".into(), "#3f7fb4".into()).unwrap();
        let n3 = store.create_note(&info.id, "三".into(), "#3fb46a".into()).unwrap();

        // 反转为 三、二、一
        store
            .reorder_notes(&info.id, vec![n3.id.clone(), n2.id.clone(), n1.id.clone()])
            .unwrap();
        let titles: Vec<String> = store
            .get(&info.id)
            .unwrap()
            .index
            .notes
            .iter()
            .map(|n| n.title.clone())
            .collect();
        assert_eq!(titles, vec!["三", "二", "一"]);

        // 部分顺序：只给出一，二之外的花笺保持原相对顺序追加在末尾
        store.reorder_notes(&info.id, vec![n1.id.clone()]).unwrap();
        let titles: Vec<String> = store
            .get(&info.id)
            .unwrap()
            .index
            .notes
            .iter()
            .map(|n| n.title.clone())
            .collect();
        assert_eq!(titles, vec!["一", "三", "二"]);

        // 关闭重开，验证顺序已持久化到 box.json
        store.close(&info.id).unwrap();
        let info2 = store.open(&path).unwrap();
        let titles: Vec<String> = store
            .get(&info2.id)
            .unwrap()
            .index
            .notes
            .iter()
            .map(|n| n.title.clone())
            .collect();
        assert_eq!(titles, vec!["一", "三", "二"]);

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
        let info = store.create(&path).unwrap();
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

    #[test]
    fn box_name_from_path_derives_file_stem() {
        assert_eq!(
            box_name_from_path(Path::new("/a/我的花匣.hxl")).as_deref(),
            Some("我的花匣")
        );
        assert_eq!(
            box_name_from_path(Path::new("/a/v1.2.hxl")).as_deref(),
            Some("v1.2")
        );
        assert_eq!(
            box_name_from_path(Path::new("/a/无后缀")).as_deref(),
            Some("无后缀")
        );
        // 没有文件名（根目录/空路径）时派生不出名字，交给兜底
        assert_eq!(box_name_from_path(Path::new("/")), None);
        assert_eq!(box_name_or_default(Path::new("/")), "未命名花匣");
    }

    #[test]
    fn sanitize_box_name_rejects_bad_input() {
        for bad in [
            "", "   ", "a/b", "a\\b", "a:b", "a?b", "a*b", "a\"b", "a<b", "a>b", "a|b", "CON",
            "con", "lpt9", "nul.txt", "名字.", ".", "..",
        ] {
            assert!(sanitize_box_name(bad).is_err(), "应当拒绝：{bad:?}");
        }
        assert!(sanitize_box_name(&"长".repeat(MAX_BOX_NAME_CHARS + 1)).is_err());
        // 首尾空白被 trim；含点号但是合法名
        assert_eq!(sanitize_box_name(" 名字 ").unwrap(), "名字");
        assert_eq!(sanitize_box_name("v1.2").unwrap(), "v1.2");
    }

    #[test]
    fn create_uses_file_name_as_box_name() {
        let dir = std::env::temp_dir().join("huajian_name_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("我的花匣.hxl");

        let mut store = BoxStore::new();
        let info = store.create(&path).unwrap();
        assert_eq!(info.name, "我的花匣");
        assert_eq!(read_index_name(&path), "我的花匣");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_uses_file_name_and_backfills_on_next_save() {
        let dir = std::env::temp_dir().join("huajian_backfill_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("正名.hxl");

        let mut store = BoxStore::new();
        let info = store.create(&path).unwrap();
        let note = store.create_note(&info.id, "笔记".into(), "#b4753f".into()).unwrap();
        store.write_note(&info.id, &note.id, "正文").unwrap();
        store.close(&info.id).unwrap();

        // 篡改 box.json 里的名字，模拟旧文件/外部改写
        tamper_index_name(&path, "旧名字");
        assert_eq!(read_index_name(&path), "旧名字");

        // 打开时名字以文件名为准；磁盘此刻仍是旧名字（懒回写）
        let info2 = store.open(&path).unwrap();
        assert_eq!(info2.name, "正名");
        assert_eq!(read_index_name(&path), "旧名字");

        // 任何一次保存都会把文件名回填进 box.json
        store.write_note(&info2.id, &note.id, "正文二").unwrap();
        assert_eq!(read_index_name(&path), "正名");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_same_path_twice_reuses_id() {
        let dir = std::env::temp_dir().join("huajian_dedupe_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("去重.hxl");

        let mut store = BoxStore::new();
        let created = store.create(&path).unwrap();
        store.close(&created.id).unwrap();

        let a = store.open(&path).unwrap();
        let b = store.open(&path).unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(store.boxes.len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_box_renames_file_and_keeps_content() {
        let dir = std::env::temp_dir().join("huajian_rename_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let old_path = dir.join("旧名.hxl");
        let new_path = dir.join("新名.hxl");

        let mut store = BoxStore::new();
        let info = store.create(&old_path).unwrap();
        let note = store.create_note(&info.id, "笔记".into(), "#b4753f".into()).unwrap();
        store.write_note(&info.id, &note.id, "内容").unwrap();

        let renamed = store.rename_box(&info.id, "新名").unwrap();
        assert_eq!(renamed.id, info.id);
        assert_eq!(renamed.name, "新名");
        assert_eq!(renamed.path, new_path.display().to_string());
        assert!(!old_path.exists());
        assert!(new_path.exists());
        // box.json 里的名字已回填
        assert_eq!(read_index_name(&new_path), "新名");
        // 正文不丢
        assert_eq!(store.read_note(&info.id, &note.id).unwrap(), "内容");

        // 关闭后用新路径能重新打开
        store.close(&info.id).unwrap();
        let reopened = store.open(&new_path).unwrap();
        assert_eq!(reopened.name, "新名");
        assert_eq!(store.read_note(&reopened.id, &note.id).unwrap(), "内容");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_box_rejects_existing_target() {
        let dir = std::env::temp_dir().join("huajian_rename_conflict_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let a_path = dir.join("a.hxl");
        let b_path = dir.join("b.hxl");

        // b 用另一个 store 建好后关掉，避免命中「已被另一个打开的花匣占用」分支
        {
            let mut other = BoxStore::new();
            other.create(&b_path).unwrap();
        }

        let mut store = BoxStore::new();
        let a = store.create(&a_path).unwrap();

        let err = store.rename_box(&a.id, "b").unwrap_err();
        assert!(err.contains("已存在"), "错误文案应提示重名：{err}");
        // 两个文件都还在，内存也未变
        assert!(a_path.exists());
        assert!(b_path.exists());
        let b = store.get(&a.id).unwrap();
        assert_eq!(b.index.name, "a");
        assert_eq!(b.path, a_path);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_box_case_only() {
        let dir = std::env::temp_dir().join("huajian_rename_case_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let old_path = dir.join("CaseTest.hxl");
        let new_path = dir.join("casetest.hxl");

        let mut store = BoxStore::new();
        let info = store.create(&old_path).unwrap();
        let note = store.create_note(&info.id, "笔记".into(), "#b4753f".into()).unwrap();
        store.write_note(&info.id, &note.id, "大小写").unwrap();

        let renamed = store.rename_box(&info.id, "casetest").unwrap();
        assert_eq!(renamed.name, "casetest");
        assert!(new_path.exists());
        assert_eq!(store.read_note(&info.id, &note.id).unwrap(), "大小写");
        // 目录里只剩一个 .hxl，且没有残留临时文件
        let hxl_count = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .to_lowercase()
                    .ends_with(".hxl")
            })
            .count();
        assert_eq!(hxl_count, 1);
        let tmp_left = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().starts_with(".hxl-rename-"));
        assert!(!tmp_left, "不应残留改名临时文件");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_box_same_name_is_noop() {
        let dir = std::env::temp_dir().join("huajian_rename_noop_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("同名.hxl");

        let mut store = BoxStore::new();
        let info = store.create(&path).unwrap();
        let renamed = store.rename_box(&info.id, "同名").unwrap();
        assert_eq!(renamed.path, info.path);
        assert_eq!(renamed.name, "同名");
        assert!(path.exists());

        let _ = fs::remove_dir_all(&dir);
    }
}

/// 回归测试：固化 0.9.3-beta 审查中实测复现的缺陷，防止复发。
///
/// 每条用例都对应一个曾经的线上 bug（多数会造成不可恢复的数据丢失），
/// 注释里写明了"修复前的表现"，改动相关代码时请勿删除。
#[cfg(test)]
mod regression {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("huajian_reg_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn read_entries(path: &Path) -> HashMap<String, Vec<u8>> {
        let bytes = fs::read(path).unwrap();
        let mut archive = Archive::new(Cursor::new(bytes));
        let mut map = HashMap::new();
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let name = entry.path().unwrap().to_string_lossy().into_owned();
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).unwrap();
            map.insert(name, buf);
        }
        map
    }

    fn write_entries(path: &Path, entries: &HashMap<String, Vec<u8>>) {
        let tmp = path.with_file_name(format!("rewrite-{}.tmp", Uuid::new_v4()));
        {
            let file = fs::File::create(&tmp).unwrap();
            let mut builder = Builder::new(file);
            builder.mode(tar::HeaderMode::Deterministic);
            let mut names: Vec<&String> = entries.keys().collect();
            names.sort();
            for name in names {
                let mut header = Header::new_gnu();
                header.set_size(entries[name].len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder
                    .append_data(&mut header, name.as_str(), entries[name].as_slice())
                    .unwrap();
            }
            builder.finish().unwrap();
        }
        fs::rename(&tmp, path).unwrap();
    }

    /// 修复前：正文里出现一行 `<!--hxl:/body-->`，重启后该行之后的内容永久消失
    #[test]
    fn body_with_end_marker_survives_restart() {
        let dir = tmpdir("body_marker");
        let path = dir.join("危险.hxl");
        let mut store = BoxStore::new();
        let info = store.create(&path).unwrap();
        let note = store.create_note(&info.id, "笔记".into(), "#b4753f".into()).unwrap();

        let body = "开头\n<!--hxl:/body-->\n结尾";
        store.write_note(&info.id, &note.id, body).unwrap();
        store.close(&info.id).unwrap();

        let info2 = store.open(&path).unwrap();
        assert_eq!(store.read_note(&info2.id, &note.id).unwrap(), body);
        let _ = fs::remove_dir_all(&dir);
    }

    /// 修复前：快照内容里出现 `<!--hxl:snapshot-end-->`，快照尾部丢失
    #[test]
    fn snapshot_with_marker_survives_restart() {
        let dir = tmpdir("snap_marker");
        let path = dir.join("快照.hxl");
        let mut store = BoxStore::new();
        let info = store.create(&path).unwrap();
        let note = store.create_note(&info.id, "笔记".into(), "#b4753f".into()).unwrap();

        let body = "正文\n<!--hxl:snapshot-end-->\n还有";
        store.write_note(&info.id, &note.id, body).unwrap();
        let meta = store.create_snapshot(&info.id, &note.id, None).unwrap();
        store.close(&info.id).unwrap();

        let info2 = store.open(&path).unwrap();
        assert_eq!(store.read_snapshot(&info2.id, &note.id, &meta.id).unwrap(), body);
        let _ = fs::remove_dir_all(&dir);
    }

    /// 修复前：仅大小写不同的改名会静默覆盖同目录下**另一个**花匣
    #[test]
    fn case_only_rename_never_overwrites_another_box() {
        let dir = tmpdir("rename_case");
        let upper = dir.join("Book.hxl");
        let lower = dir.join("book.hxl");

        let mut s1 = BoxStore::new();
        let i1 = s1.create(&upper).unwrap();
        let n1 = s1.create_note(&i1.id, "大写".into(), "#b4753f".into()).unwrap();
        s1.write_note(&i1.id, &n1.id, "UPPER-CONTENT").unwrap();

        let mut s2 = BoxStore::new();
        // 大小写不敏感文件系统上第二个文件建不出来，本用例不适用，直接跳过
        let Ok(i2) = s2.create(&lower) else {
            let _ = fs::remove_dir_all(&dir);
            return;
        };
        let n2 = s2.create_note(&i2.id, "小写".into(), "#b4753f".into()).unwrap();
        s2.write_note(&i2.id, &n2.id, "lower-content").unwrap();

        let mut store = BoxStore::new();
        let info = store.open(&upper).unwrap();
        let err = store.rename_box(&info.id, "book").unwrap_err();
        assert!(err.contains("已存在"), "{err}");

        // 两个文件都还在，小写花匣的内容没被覆盖
        assert!(upper.exists() && lower.exists());
        let mut check = BoxStore::new();
        let ci = check.open(&lower).unwrap();
        assert_eq!(check.read_note(&ci.id, &n2.id).unwrap(), "lower-content");

        let _ = fs::remove_dir_all(&dir);
    }

    /// 修复前：`same_path` 无条件小写，Linux 上 `A.hxl` 与 `a.hxl` 被合并成同一个花匣
    #[test]
    fn case_sensitive_paths_are_distinct_boxes() {
        let dir = tmpdir("case_distinct");
        let upper = dir.join("A.hxl");
        let lower = dir.join("a.hxl");

        let mut s1 = BoxStore::new();
        s1.create(&upper).unwrap();
        let mut s2 = BoxStore::new();
        let Ok(_) = s2.create(&lower) else {
            let _ = fs::remove_dir_all(&dir);
            return;
        };

        let mut store = BoxStore::new();
        let a = store.open(&upper).unwrap();
        let b = store.open(&lower).unwrap();
        assert_ne!(a.id, b.id, "A.hxl 与 a.hxl 是两个文件，不能合并");
        assert_eq!(store.boxes.len(), 2);

        let _ = fs::remove_dir_all(&dir);
    }

    /// 修复前：create 只查磁盘不查已打开，同一路径会出现两个 OpenBox 互相覆盖
    #[test]
    fn create_rejects_already_open_path() {
        let dir = tmpdir("dup_create");
        let path = dir.join("dup.hxl");
        let mut store = BoxStore::new();
        store.create(&path).unwrap();
        // 文件被外部删掉，但内存里仍处于打开状态
        fs::remove_file(&path).unwrap();
        let err = store.create(&path).unwrap_err();
        assert!(err.contains("已打开"), "{err}");
        assert_eq!(store.boxes.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    /// 修复前：版本号 = 数量 + 1，删掉中间版本后再新建会出现两个 v3
    #[test]
    fn snapshot_versions_stay_unique_after_delete() {
        let dir = tmpdir("snap_version");
        let path = dir.join("ver.hxl");
        let mut store = BoxStore::new();
        let info = store.create(&path).unwrap();
        let note = store.create_note(&info.id, "n".into(), "#b4753f".into()).unwrap();

        store.write_note(&info.id, &note.id, "v1").unwrap();
        let s1 = store.create_snapshot(&info.id, &note.id, None).unwrap();
        store.write_note(&info.id, &note.id, "v2").unwrap();
        let s2 = store.create_snapshot(&info.id, &note.id, None).unwrap();
        store.write_note(&info.id, &note.id, "v3").unwrap();
        let s3 = store.create_snapshot(&info.id, &note.id, None).unwrap();

        store.delete_snapshot(&info.id, &note.id, &s2.id).unwrap();
        store.write_note(&info.id, &note.id, "v4").unwrap();
        let s4 = store.create_snapshot(&info.id, &note.id, None).unwrap();

        let mut versions = vec![s1.version, s3.version, s4.version];
        versions.sort_unstable();
        versions.dedup();
        assert_eq!(versions.len(), 3, "版本号重复");
        let _ = fs::remove_dir_all(&dir);
    }

    /// 修复前：删除不存在的快照会先改 updated_at 再报错（失败也污染内存）
    #[test]
    fn failed_delete_snapshot_keeps_state() {
        let dir = tmpdir("dirty_state");
        let path = dir.join("d.hxl");
        let mut store = BoxStore::new();
        let info = store.create(&path).unwrap();
        let note = store.create_note(&info.id, "n".into(), "#b4753f".into()).unwrap();
        store.write_note(&info.id, &note.id, "x").unwrap();

        let before = store.get(&info.id).unwrap().index.notes[0].updated_at.clone();
        // `now()` 只精确到秒，等过一秒才能观察到"是否被改过"
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert!(store.delete_snapshot(&info.id, &note.id, "不存在").is_err());
        let after = store.get(&info.id).unwrap().index.notes[0].updated_at.clone();
        assert_eq!(before, after, "失败的操作不应改动任何状态");
        let _ = fs::remove_dir_all(&dir);
    }

    /// 修复前：`repack` 用 `<stem>.hxl.tmp`，会撞上同目录下另一个真实花匣并把它删掉
    #[test]
    fn repack_tmp_does_not_collide_with_other_box() {
        let dir = tmpdir("tmp_name");
        let a = dir.join("a.hxl");
        let b = dir.join("a.hxl.tmp"); // 另一个花匣，恰好占用了旧的临时文件名

        let mut s1 = BoxStore::new();
        let i1 = s1.create(&a).unwrap();
        let n1 = s1.create_note(&i1.id, "A".into(), "#000000".into()).unwrap();
        s1.write_note(&i1.id, &n1.id, "AAA").unwrap();

        let mut s2 = BoxStore::new();
        let i2 = s2.create(&b).unwrap();
        let n2 = s2.create_note(&i2.id, "B".into(), "#000000".into()).unwrap();
        s2.write_note(&i2.id, &n2.id, "BBB").unwrap();

        s1.write_note(&i1.id, &n1.id, "AAA2").unwrap();
        assert!(b.exists(), "保存 A 时不能动到同目录的另一个花匣");
        let mut check = BoxStore::new();
        let ci = check.open(&b).unwrap();
        assert_eq!(check.read_note(&ci.id, &n2.id).unwrap(), "BBB");
        let _ = fs::remove_dir_all(&dir);
    }

    /// 修复前：`note.file` 指向 `box.json` 的花匣会被写坏，且从此再也打不开
    #[test]
    fn malicious_note_file_cannot_brick_box() {
        let dir = tmpdir("note_file");
        let path = dir.join("evil.hxl");
        {
            let mut s = BoxStore::new();
            s.create(&path).unwrap();
        }
        let mut entries = read_entries(&path);
        let mut index: serde_json::Value = serde_json::from_slice(&entries[INDEX_FILE]).unwrap();
        index["notes"] = serde_json::json!([{
            "id": "evil",
            "title": "坏",
            "color": "#000000",
            "file": "box.json",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z"
        }]);
        entries.insert(INDEX_FILE.into(), serde_json::to_vec_pretty(&index).unwrap());
        write_entries(&path, &entries);

        let mut store = BoxStore::new();
        let info = store.open(&path).unwrap();
        assert_eq!(
            store.get(&info.id).unwrap().index.notes[0].file,
            "notes/evil.md",
            "不安全的 note.file 应被改写"
        );
        store.write_note(&info.id, "evil", "正文").unwrap();
        store.close(&info.id).unwrap();

        // 关键：保存之后花匣仍然能正常打开
        let mut again = BoxStore::new();
        let info2 = again.open(&path).expect("修复后花匣不应被写坏");
        assert_eq!(again.read_note(&info2.id, "evil").unwrap(), "正文");
        let _ = fs::remove_dir_all(&dir);
    }

    /// 修复前：`box_name_from_path(".hxl")` 返回 Some(".hxl")，会造出看不见的花匣
    #[test]
    fn dot_hxl_has_no_name() {
        assert_eq!(box_name_from_path(Path::new("/tmp/x/.hxl")), None);
        assert_eq!(box_name_from_path(Path::new("/")), None);
        assert_eq!(box_name_from_path(Path::new("/tmp/x/正常.hxl")), Some("正常".into()));
    }

    /// 修复前：允许名字里再带 `.hxl`，最终得到 `笔记.hxl.hxl`
    #[test]
    fn sanitize_rejects_double_extension() {
        assert!(sanitize_box_name("笔记.hxl").is_err());
        assert!(sanitize_box_name("笔记.HXL").is_err());
        assert!(sanitize_box_name("v1.2").is_ok());
    }

    /// 修复前：序列化 `trim_end` 把尾部空白（硬换行/空行）吃掉
    #[test]
    fn trailing_whitespace_survives_restart() {
        let dir = tmpdir("trailing");
        let path = dir.join("空白.hxl");
        let mut store = BoxStore::new();
        let info = store.create(&path).unwrap();
        let note = store.create_note(&info.id, "n".into(), "#b4753f".into()).unwrap();

        let body = "第一行  \n第二行\n\n";
        store.write_note(&info.id, &note.id, body).unwrap();
        store.close(&info.id).unwrap();

        let info2 = store.open(&path).unwrap();
        assert_eq!(store.read_note(&info2.id, &note.id).unwrap(), body);
        let _ = fs::remove_dir_all(&dir);
    }

    /// 内容未变化时不重打包（切标签不再无谓写盘）
    #[test]
    fn unchanged_write_does_not_repack() {
        let dir = tmpdir("noop_write");
        let path = dir.join("noop.hxl");
        let mut store = BoxStore::new();
        let info = store.create(&path).unwrap();
        let note = store.create_note(&info.id, "n".into(), "#b4753f".into()).unwrap();
        store.write_note(&info.id, &note.id, "内容").unwrap();
        store.close(&info.id).unwrap();

        // 重新打开：backup_done = false，只要发生一次 repack 就会写出 .bak
        let info2 = store.open(&path).unwrap();
        store.write_note(&info2.id, &note.id, "内容").unwrap();
        assert!(
            !path.with_extension("hxl.bak").exists(),
            "内容没变不应重打包"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// 第一次真正保存时留下 `.hxl.bak`，给整包损坏留一条退路
    #[test]
    fn first_real_save_creates_backup() {
        let dir = tmpdir("bak");
        let path = dir.join("bak.hxl");
        let mut store = BoxStore::new();
        let info = store.create(&path).unwrap();
        let note = store.create_note(&info.id, "n".into(), "#b4753f".into()).unwrap();
        store.write_note(&info.id, &note.id, "第一版").unwrap();
        store.close(&info.id).unwrap();

        let before = fs::read(&path).unwrap();
        let info2 = store.open(&path).unwrap();
        store.write_note(&info2.id, &note.id, "第二版").unwrap();

        let bak = path.with_extension("hxl.bak");
        assert!(bak.exists(), "第一次真实保存应留下备份");
        assert_eq!(fs::read(&bak).unwrap(), before, "备份应是覆盖前的内容");
        let _ = fs::remove_dir_all(&dir);
    }

    /// 删除不存在的花笺必须报错，不能静默成功
    #[test]
    fn delete_missing_note_errors() {
        let dir = tmpdir("del_missing");
        let path = dir.join("m.hxl");
        let mut store = BoxStore::new();
        let info = store.create(&path).unwrap();
        assert!(store.delete_note(&info.id, "不存在").is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
