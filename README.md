# 花笺 infinity

所见即所得 Markdown 笔记，笔记以「花匣」为容器打包成单文件 `.hxl`（tar 套 json，可整体拷贝到其它设备）。
内置番茄钟（花钟）功能，番茄钟数据与笔记数据分开存放，可与独立的「花钟」应用共用同一份数据。

- 版本：`0.9.0-beta`
- 许可：MulanPSL-2.0（见 `LICENSE`）

## 功能

- 花匣（`.hxl`）单文件笔记包：花笺增删改、排序、颜色、修改历史与快照回退
- Markdown 所见即所得编辑（Milkdown）、导出 Markdown / PDF
- 八套主题、明暗两版、正文宽度两档、侧栏宽度可调
- 番茄钟：标准倒计时 / 正向正计时、休息循环、按日统计与日历热力
- 番茄钟记录管理：可删除任意记录、可缩短已完成番茄的时长（**只能减短**，不提供手工新增）

## 用户数据位置

数据根目录由系统配置目录推导（**与 app 标识符无关**，整个目录可拷贝迁移）：

```
Linux   ~/.config/huajian/
Windows %APPDATA%\huajian
macOS   ~/Library/Application Support/huajian
├── 花笺/
│   └── settings.json      # 主题、明暗、侧栏宽度、正文宽度、上次打开的花匣列表
└── 花钟.hz                # 番茄钟全部数据：无压缩 tar 套一个 huazhong.json
```

- 应用内「番茄记录」面板底部有「打开数据位置」按钮。
- 备份 / 跨设备搬移只需拷 `花钟.hz` 一个文件；格式自包含（只存本地墙上时刻，不含时区/时间戳/绝对路径）。
- 手工查看：`tar -xOf 花钟.hz huazhong.json | python3 -m json.tool`
- 从旧版本（`~/.config/com.huajian.app/`）升级时，首次启动会自动导入旧设置与统计，
  并把旧文件改名为 `*.migrated-YYYYMMDD` 留作备份。
- 数据格式契约见 [`docs/huazhong-format.md`](docs/huazhong-format.md)。

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
src/                     Svelte 5 前端（组件、编辑器、主题、番茄钟面板）
src/lib/pomodoro-core.ts 番茄钟状态机（纯逻辑，可被独立「花钟」应用复用）
src-tauri/src/           Rust 后端
  box_store.rs           花匣 .hxl 打包/解包
  note_doc.rs            笔记正文与快照的序列化
  huazhong.rs            花钟数据层（tar 读写、区间并集、互斥锁、增删改）
  paths.rs               用户数据目录布局 + 旧数据一次性迁移
  settings.rs            应用设置
  commands.rs            Tauri 命令
docs/huazhong-format.md  花钟数据格式契约（与独立「花钟」应用共用）
scripts/                 构建与自测脚本
```