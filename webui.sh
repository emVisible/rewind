#!/usr/bin/env bash
# Rewind 本地 Web UI —— 调试/测试用一条命令
#
#   ./webui.sh                 # 起在 8137,浏览器开 http://127.0.0.1:8137/
#   ./webui.sh 8200            # 换端口
#   ./webui.sh --rebuild       # 先 cargo build --release core 再起
#   REWIND_FFMPEG=/path/ffmpeg ./webui.sh   # 指定引擎用的 ffmpeg
#
# 界面文件就是 app/ui 那一套(与桌面壳共用),Ctrl-C 直接退进程。
set -uo pipefail
cd "$(dirname "$0")" || exit 1
ROOT=$PWD

PORT=${REWIND_PORT:-8137}
REBUILD=0
for a in "$@"; do
  case "$a" in
    --rebuild|-r) REBUILD=1 ;;
    --help|-h)    sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *[!0-9]*)     echo "参数看不懂: $a  (端口给数字,其他只认 --rebuild / --help)"; exit 2 ;;
    *)            PORT=$a ;;
  esac
done

BIN=$ROOT/core/target/release/rewind-core
[ -x "$BIN" ] || BIN=$ROOT/core/target/release/rewind-core.exe

if [ "$REBUILD" = 1 ]; then
  echo "构建引擎(core, release)…"
  (cd "$ROOT/core" && cargo build --release) || { echo "构建失败"; exit 1; }
fi

if [ ! -x "$BIN" ]; then
  echo "找不到引擎二进制: $BIN"
  echo "先跑: ./webui.sh --rebuild   (或 cd core && cargo build --release)"
  exit 1
fi

# 引擎按 PATH 找 ffmpeg/ffprobe(可用 REWIND_FFMPEG / REWIND_FFPROBE 指路径),起不来就做不出成品
FF=${REWIND_FFMPEG:-ffmpeg}
FP=${REWIND_FFPROBE:-ffprobe}
for b in "$FF" "$FP"; do
  if ! command -v "$b" >/dev/null 2>&1 && [ ! -x "$b" ]; then
    echo "缺 $b —— 上传后会在探测那步失败。装一个或指路径:"
    echo "  sudo apt install ffmpeg"
    echo "  REWIND_FFMPEG=/path/ffmpeg REWIND_FFPROBE=/path/ffprobe ./webui.sh"
    exit 1
  fi
done

# 端口被占通常是上一次没退出的实例:让它退出比让 serve 报一个难读的 bind 错误有用
if command -v ss >/dev/null 2>&1 && ss -ltn 2>/dev/null | grep -qE ":$PORT[[:space:]]"; then
  echo "端口 $PORT 已被占用(多半是上次没关的实例)。"
  echo "  接着用: 直接开 http://127.0.0.1:$PORT/"
  echo "  换端口: ./webui.sh $((PORT + 1))"
  echo "  杀掉旧的: fuser -k $PORT/tcp"
  exit 1
fi

echo "引擎 : $BIN"
echo "UI   : $ROOT/app/ui"
echo "预设 : $ROOT/presets"
[ -f "$ROOT/fixtures/test_src.mp4" ] && echo "备测 : $ROOT/fixtures/test_src.mp4(页面里可直接添加)"
echo "地址 : http://127.0.0.1:$PORT/   (Ctrl-C 退出)"
echo
exec "$BIN" serve --port "$PORT"
