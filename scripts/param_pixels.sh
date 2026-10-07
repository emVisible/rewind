#!/usr/bin/env bash
# 像素活性闸:每个视觉参数单独拧动必须改变成品帧(与 param_liveness 的结构层对账互补)。
# 用法: bash scripts/param_pixels.sh   (约 4-6 分钟:每对取值两次小预览)
set -uo pipefail
cd "$(dirname "$0")/.."
BIN=./core/target/release/rewind-core
[ -x "$BIN" ] || BIN=./core/target/release/rewind-core.exe
[ -x "$BIN" ] || { echo "FATAL 先 cargo build --release core"; exit 2; }
command -v python3 >/dev/null 2>&1 || { echo "SKIP  缺 python3"; exit 0; }
python3 scripts/param_pixels.py
rc=$?
[ $rc -eq 0 ] || echo "修法:两帧一样 = 这个参数改不到画面 —— 接上它,或从清单里摘掉(音频类参数请归到音频断言那边)。"
exit $rc
