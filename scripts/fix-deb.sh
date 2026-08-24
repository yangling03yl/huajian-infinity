#!/bin/sh
# 修复 tauri 构建的 deb：tauri 将产品名（中文）直接用作 Package 字段，
# dpkg 要求包名以字母/数字开头，此处统一改为 ASCII 包名 huajian。
# 用法: scripts/fix-deb.sh <输入.deb> <输出.deb>
set -e

in="$1"
out="$2"

if [ -z "$in" ] || [ -z "$out" ]; then
  echo "用法: $0 <输入.deb> <输出.deb>" >&2
  exit 1
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

dpkg-deb -R "$in" "$tmp"

pkg="$(awk '/^Package:/{print $2; exit}' "$tmp/DEBIAN/control")"
case "$pkg" in
  *[!a-zA-Z0-9+.-]* | '' | -* | +* | .* | *.*.*)
    sed -i 's/^Package: .*/Package: huajian/' "$tmp/DEBIAN/control"
    echo "包名已从「$pkg」修正为 huajian"
    ;;
  *)
    echo "包名 $pkg 合法，无需修正"
    ;;
esac

dpkg-deb -b "$tmp" "$out" >/dev/null
echo "已生成: $out"
