# 花笺 infinity

所见即所得 Markdown 笔记，笔记以「花匣」为容器打包成单文件 `.hxl`（tar 套 json，可整体拷贝到其它设备）。

- 版本：`1.0.0-rc2`
- 许可：MulanPSL-2.0（见 `LICENSE`）

## 功能

- 花匣（`.hxl`）单文件笔记包：花笺增删改、排序、颜色、修改历史与快照回退
- 花匣名即文件名（`.hxl` 的无后缀文件名），侧栏可重命名（双击花匣名或点 ✎，会真改磁盘文件名）
- Markdown 所见即所得编辑（Milkdown）、导出 Markdown / PDF
- 八套主题、明暗两版、正文宽度两档、侧栏宽度可调
- 段落样式：每个段落只有「标题级别 + 字号」一套样式。默认是正文；`Enter` 新建段落
  一律回到默认样式，`Shift+Enter` 的段内软换行保留当前样式；标题字号由级别固定，
  只有正文段落可调字号

## 用户数据位置

数据根目录由系统配置目录推导（**与 app 标识符无关**，整个目录可拷贝迁移）：

```
Linux   ~/.config/huajian/
Windows %APPDATA%\huajian
macOS   ~/Library/Application Support/huajian
└── 花笺/
    └── settings.json      # 主题、明暗、侧栏宽度、正文宽度、上次打开的花匣列表
```

- 从旧版本（`~/.config/com.huajian.app/`）升级时，首次启动会自动导入旧设置，
  并把旧文件改名为 `*.migrated-YYYYMMDD` 留作备份。

## 数据安全说明

- 花匣保存采用「写临时文件 → `fsync` → 原子改名」，临时文件形如
  `.hxl-tmp-<uuid>.tmp`（保存结束后自动清理）。
- 每次会话**第一次**真正写入某个花匣时，会把原文件复制一份 `<花匣名>.hxl.bak`，
  用于整包损坏时的回落；确认无误后可自行删除。
- `.hxl` 内部的正文/快照使用 HTML 注释分帧，用户内容里出现同类文本时会做行首转义，
  保存与读取严格可逆；旧版本生成的花匣可直接打开。
- 段落字号是**段落属性**，Markdown 里以整段 `<span style="font-size: Npx">…</span>`
  表示。旧花匣中「只给半句话设字号」的行内 span 会在读取时提升为整段字号
  （取覆盖字符数最多的那个），不会改坏原文；只有用户再次编辑并保存时才按新格式写回。
- 快照预览与 PDF 导出会禁用 Markdown 里的原始 HTML 并过滤危险链接协议，
  因此打开他人分享的花匣不会执行其中的脚本。

## 构建

### 前置

- Node.js 20+ 与 npm
- Rust 稳定版工具链（`rustup`）
- Linux 还需系统依赖：`webkit2gtk-4.1`、`libgtk-3-dev`、`libayatana-appindicator3-dev`、`librsvg2-dev`、`build-essential`、`pkg-config`、`libssl-dev`

### 命令

```bash
npm ci                  # 按 package-lock.json 安装前端依赖
npm run dev             # 仅前端开发服务器（http://localhost:1420）
npm run check           # svelte-check 类型检查
npm run build           # 仅构建前端到 dist/

npm run tauri dev       # 桌面端联调（自动拉起 vite）
npm run tauri build     # 构建当前平台的安装包
cargo test --manifest-path src-tauri/Cargo.toml   # 运行 Rust 单元测试
```

### 各平台产物

| 平台 | 命令 | 产物 |
|---|---|---|
| Linux x86_64 | `npm run tauri build`（或 `--target x86_64-unknown-linux-gnu --bundles deb`） | `*.deb`（amd64） |
| Linux aarch64 | `npm run tauri build -- --target aarch64-unknown-linux-gnu --bundles deb` | `*.deb`（arm64） |
| Linux loongarch64 | 见下方说明 | `*.deb`（loongarch64） |
| Windows x64 / arm64 | 在 Windows 上 `npm run tauri build` | `*-setup.exe`（NSIS）与 `*.msi` |

说明：

- 交叉编译 Linux aarch64 / loongarch64 需要对应 sysroot 与交叉工具链
  （仓库不包含 sysroot）；loongarch64 还需要一个上一版的 deb 作为打包模板，
  可用 `scripts/build-loongarch64.sh <模板.deb> [输出目录]`（脚本会校验二进制内嵌版本串与体积，防止陈旧产物）。
- Windows 在 Linux 上交叉构建需要 `cargo-xwin` + NSIS / WiX，本仓库不附带这套环境。
- tauri 构建会自动执行 `beforeBuildCommand`（即 `npm run build`），因此改前端无需手动先 build。

## 目录结构

```
src/                     Svelte 5 前端（组件、编辑器、主题）
src-tauri/src/           Rust 后端
  box_store.rs           花匣 .hxl 打包/解包
  note_doc.rs            笔记正文与快照的序列化
  paths.rs               用户数据目录布局 + 旧数据一次性迁移
  settings.rs            应用设置
  commands.rs            Tauri 命令
scripts/                 构建与自测脚本
```
