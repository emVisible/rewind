#!/usr/bin/env bash
# 参数活性闸:清单里的每个参数都必须能在 rewind-core plan 里留下可见痕迹。
# 这条闸防的是"界面上有控件、引擎里没人吃这个值"(历史上真发生过:ntsc_vhs.settings)。
# 用法: bash scripts/param_liveness.sh
set -uo pipefail
cd "$(dirname "$0")/.."
BIN=./core/target/release/rewind-core
[ -x "$BIN" ] || BIN=./core/target/release/rewind-core.exe
[ -x "$BIN" ] || { echo "FATAL 先 cargo build --release core"; exit 2; }
command -v python3 >/dev/null 2>&1 || { echo "SKIP  缺 python3"; exit 0; }
python3 scripts/param_liveness.py
rc=$?
[ $rc -eq 0 ] || echo "修法:参数若在引擎里真的起作用,计划文本必然变;不变就是没接线 —— 要么接上,要么从清单里摘掉。"
exit $rc
