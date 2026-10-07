#!/usr/bin/env bash
# CI 包装:跑一条命令,失败时把关键行变成 check-run 注解。
# 为什么要这个:Actions 的日志下载要认证(未认证 403),而注解走 /check-runs/{id}/annotations
# 公开仓库不登录也能读 —— 于是"CI 红了"这件事可以被脚本自己说清楚,不必人去翻页面。
# 用法: bash scripts/ci_step.sh <命令...>
set -uo pipefail
log=$(mktemp)
"$@" >"$log" 2>&1
rc=$?
cat "$log"
if [ $rc -ne 0 ]; then
  # 消息里的 "::" 与 "%" 会被 GitHub 当作命令语法,必须转义(连命令本身一起转,它常含冒号)
  esc() { printf '%s' "$1" | tr -d '\r' | sed 's/%/%25/g; s/::/%3A%3A/g' | cut -c1-240; }
  printf '::error title=步骤失败::%s\n' "$(esc "退出码 $rc;命令:$*")"
  grep -aE "^FAIL |^PARAM [A-Z]+: (FAIL|SKIP)|\.\.\. FAILED|test result: FAILED|^error(\[E[0-9]+\])?:" "$log" \
    | head -24 \
    | while IFS= read -r l; do
        printf '::error::%s\n' "$(esc "$l")"
      done
  # panic 的正文在 "panicked at" 的下一行,只抓一行等于没抓 —— 带上下文再发一遍
  sed -n '/panicked at/,+1p' "$log" | head -24 | while IFS= read -r l; do
    printf '::error::%s\n' "$(esc "$l")"
  done
  # 最后几行兜底(权限、段错误这类没有固定形状的失败)
  tail -4 "$log" | while IFS= read -r l; do printf '::error::尾部 %s\n' "$(esc "$l")"; done
fi
exit $rc
