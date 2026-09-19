#!/bin/bash
# loongarch64 交叉构建 + 手工打 deb 一键脚本（tauri-bundler 不支持 LoongArch，必须手工打包）
#
# 用法:  scripts/build-loongarch64.sh <模板deb> [输出目录]
#   <模板deb>   上一版的 loongarch64 deb，用作 control/data 结构模板
#               （如 dist/V0.6.0-beta/huajian-infinity_0.6.0-beta_loongarch64.deb）
#   [输出目录]  默认 dist/（仓库外的工作区 dist）
#
# 内置防陈旧产物保护：
#   1. 构建前强制 cargo clean -p huajian --target <triple>
#   2. 构建后用 strings 校验二进制内嵌版本串 == tauri.conf.json 的 version
#   3. 校验二进制体积 >= 10MB（防止漏 --features tauri/custom-protocol 导致资源未嵌入/白屏）
set -euo pipefail

TEMPLATE="${1:?用法: $0 <模板deb> [输出目录]}"
OUTDIR="${2:-}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/src-tauri"

# ---- 版本号（唯一来源：tauri.conf.json）----
VERSION="$(grep -m1 '"version"' tauri.conf.json | sed 's/.*: "\([^"]*\)".*/\1/')"
echo "== 版本: $VERSION"

# ---- 交叉编译环境 ----
# 包装脚本（pkg-config/linker + rpath/auto-libs）现位于仓库外工作区的 .cross-env/bin
# （原 /mnt/data1/huajian-cross 已随磁盘整理移除；loong64 sysroot 仍在 /var/tmp/aosc-loong64-sysroot）
CROSS_BIN="/mnt/data1/写作和文件/花笺/花笺无限/.cross-env/bin"
export CARGO_TARGET_LOONGARCH64_UNKNOWN_LINUX_GNU_LINKER="$CROSS_BIN/linker-loong64"
export PKG_CONFIG="$CROSS_BIN/pkg-config-loong64"
export PKG_CONFIG_ALLOW_CROSS=1
export CC_loongarch64_unknown_linux_gnu=loongarch64-linux-gnu-gcc
export CXX_loongarch64_unknown_linux_gnu=loongarch64-linux-gnu-g++
export AR_loongarch64_unknown_linux_gnu=loongarch64-linux-gnu-ar

# ---- 防陈旧：清掉本包缓存再编 ----
cargo clean -p huajian --target loongarch64-unknown-linux-gnu

# 注意：直接 cargo build 的输出名是 bin name "huajian"；
# "huajian-infinity" 是 tauri CLI 复制的副本，这里绕开 tauri CLI 直接取 huajian。
cargo build --release --target loongarch64-unknown-linux-gnu --features tauri/custom-protocol

BIN="target/loongarch64-unknown-linux-gnu/release/huajian"

# ---- 防陈旧校验 1：内嵌版本串 ----
EMBEDDED="$(strings -a "$BIN" | grep -oE '[0-9]+\.[0-9]+\.[0-9]+-beta' | sort -u | tail -1 || true)"
if [ "$EMBEDDED" != "$VERSION" ]; then
  echo "错误: 二进制内嵌版本($EMBEDDED) != 期望($VERSION)，存在陈旧产物，中止" >&2
  exit 1
fi

# ---- 防陈旧校验 2：体积（资源已嵌入应 ~17MB）----
SIZE="$(stat -c%s "$BIN")"
if [ "$SIZE" -lt 10000000 ]; then
  echo "错误: 二进制仅 ${SIZE} 字节，疑似漏 --features tauri/custom-protocol，中止" >&2
  exit 1
fi
echo "== 二进制校验通过: $(file -b "$BIN" | cut -d, -f1-2), ${SIZE} 字节, 版本 $EMBEDDED"

# ---- 手工打 deb（以模板包为骨架）----
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/tpl" && cd "$STAGE/tpl"
ar x "$TEMPLATE"
mkdir ctl dat && tar xzf control.tar.gz -C ctl && tar xzf data.tar.gz -C dat

# 替换二进制（deb 内命名为 huajian-infinity，与官方包一致）
install -m 755 "$ROOT/src-tauri/$BIN" dat/usr/bin/huajian-infinity

# control：更新 Version 与 Installed-Size
sed -i "s/^Version: .*/Version: $VERSION/" ctl/control
INSTALLED_SIZE="$(du -sk dat | cut -f1)"
sed -i "s/^Installed-Size: .*/Installed-Size: $INSTALLED_SIZE/" ctl/control

# md5sums 全量重算（路径格式跟随模板：不带前导 ./）
( cd dat && find . -type f ! -name DEBIAN -printf '%P\n' | sort | xargs md5sum ) > ctl/md5sums

# 重新打包（条目保持 ./ 前缀格式，root 属主）
tar czf control.tar.gz -C ctl . --owner=root --group=root
tar czf data.tar.gz -C dat . --owner=root --group=root
# debian-binary 成员的内容是格式版本号，不是成员名
printf '2.0\n' > debian-binary
DEB="huajian-infinity_${VERSION}_loongarch64.deb"
ar r "$DEB" debian-binary control.tar.gz data.tar.gz

# ---- 出包自检 ----
ar t "$DEB" | head -3
dpkg-deb --info "$DEB" | grep -E '^( Version| Architecture| Installed-Size)'

# ---- 输出 ----
if [ -z "$OUTDIR" ]; then
  OUTDIR="$(dirname "$ROOT")/dist/V$VERSION"
fi
mkdir -p "$OUTDIR"
cp "$DEB" "$OUTDIR/"
echo "== 完成: $OUTDIR/$DEB"
