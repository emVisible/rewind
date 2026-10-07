#!/bin/sh
# 假 ffmpeg:前 N-1 次调用放行(交回真 ffmpeg 产出),第 N 次开始失败。
# 用来复现"跑到中间某一趟才炸"的场景 —— 这时上一趟的中间件该不该被清掉,只有这条路能量。
STATE="${FAKE_FAIL_STATE:-/tmp/fake_ff_state}"
CNT=$(cat "$STATE" 2>/dev/null || echo 0)
# 只有带 -i 的才算一次真正的编码调用(探针等不算)
IS_ENCODE=0
for a in "$@"; do
  [ "$a" = "-i" ] && IS_ENCODE=1
done
if [ "$IS_ENCODE" = "1" ]; then
  CNT=$((CNT + 1))
  echo "$CNT" > "$STATE"
fi
if [ "$IS_ENCODE" = "1" ] && [ "$CNT" -ge "${FAKE_FAIL_AT:-3}" ]; then
  echo "fake ffmpeg: 第 $CNT 次调用被人为点掉(bogus filter boom)" >&2
  exit 1
fi
exec /usr/bin/env ffmpeg "$@"
