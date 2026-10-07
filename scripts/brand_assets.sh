#!/usr/bin/env bash
# 从一张母图重生成全部品牌资产:网页 logo/favicon、Tauri 图标集。
# 母图是唯一真源;派生文件全部入库,免得打包时还要跑一次生成。
set -euo pipefail
cd "$(dirname "$0")/.."
SRC=assets/brand/source-icon.png
[ -f "$SRC" ] || { echo "FATAL: 缺母图 $SRC"; exit 2; }
command -v ffmpeg >/dev/null || { echo "FATAL: 需要 ffmpeg"; exit 2; }

mk() { # 尺寸 输出
  ffmpeg -v error -y -i "$SRC" -vf "scale=$1:$1:flags=lanczos" "$2"
  echo "  $2"
}

mkdir -p app/ui/img app/icons
echo "=== 网页资产"
mk 96  app/ui/img/logo.png
mk 64  app/ui/img/logo-64.png
mk 32  app/ui/img/favicon-32.png
mk 180 app/ui/img/apple-touch-icon.png
ffmpeg -v error -y -i "$SRC" -vf 'scale=48:48:flags=lanczos' /tmp/_i48.png
ffmpeg -v error -y -i "$SRC" -vf 'scale=32:32:flags=lanczos' /tmp/_i32.png
ffmpeg -v error -y -i "$SRC" -vf 'scale=16:16:flags=lanczos' /tmp/_i16.png
ffmpeg -v error -y -i /tmp/_i48.png -i /tmp/_i32.png -i /tmp/_i16.png \
  -map 0 -map 1 -map 2 app/ui/favicon.ico && echo "  app/ui/favicon.ico"

echo "=== Tauri 图标集(打包要读 app/icons/,少一个就构建失败)"
mk 32  app/icons/32x32.png
mk 128 app/icons/128x128.png
mk 256 'app/icons/128x128@2x.png'
mk 512 app/icons/icon.png
ffmpeg -v error -y -i /tmp/_i48.png -i /tmp/_i32.png -i /tmp/_i16.png \
  -map 0 -map 1 -map 2 app/icons/icon.ico && echo "  app/icons/icon.ico"
rm -f /tmp/_i48.png /tmp/_i32.png /tmp/_i16.png

echo "=== 清单"
ls -la app/ui/img app/icons | grep -v '^total\|^d'
du -ch app/ui/img/* app/icons/* app/ui/favicon.ico 2>/dev/null | tail -1
