#!/usr/bin/env bash
# 音频活性闸:每个会影响声音的参数单独拧动,必须改变成品音轨(抽成 f64 裸流比 md5)。
# 与 param_pixels 互补:那条比画面,这条比声音 —— 两条合起来才没有"拧了没反应"的死角。
# 用法: bash scripts/param_audio.sh
set -uo pipefail
cd "$(dirname "$0")/.."
BIN=./core/target/release/rewind-core
[ -x "$BIN" ] || BIN=./core/target/release/rewind-core.exe
[ -x "$BIN" ] || { echo "FATAL 先 cargo build --release core"; exit 2; }
command -v python3 >/dev/null 2>&1 || { echo "SKIP  缺 python3"; exit 0; }
command -v ffprobe >/dev/null 2>&1 || { echo "SKIP  缺 ffprobe"; exit 0; }
python3 scripts/param_audio.py
rc=$?
[ $rc -eq 0 ] || echo "修法:音轨没变 = 这个参数没接到声音链上 —— 要么接上,要么从清单里摘掉。"
exit $rc
