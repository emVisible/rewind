#!/usr/bin/env bash
# CI 包装:跑一条命令,失败时把关键行变成 check-run 注解。
# 为什么要这个:Actions 的日志下载要认证(未认证 403),而注解走 /check-runs/{id}/annotations
# 公开仓库不登录也能读 —— 于是"CI 红了"这件事可以被脚本自己说清楚,不必人去翻页面。
# 用法: bash scripts/ci_step.sh <命令...>
set -uo pipefail
log=$(mktemp)
# 自己认解释器:调用方写 `ci_step.sh scripts/regression.sh` 就够。
# 不这么做的话,CI 里就会出现"包一层 bash 之后里面那个 bash 丢了"的形状 ——
# 直接执行一个 644 的 .sh 得到 exit 126(Permission denied),报的还是包装层的错。
case "${1:-}" in
  *.sh) run_cmd=(bash "$@") ;;
  *.py) run_cmd=(python3 "$@") ;;
  *.js) run_cmd=(node "$@") ;;
  *)    run_cmd=("$@") ;;
esac
"${run_cmd[@]}" >"$log" 2>&1
rc=$?
cat "$log"
if [ $rc -ne 0 ]; then
  # 消息里的 "::" 与 "%" 会被 GitHub 当作命令语法,必须转义(连命令本身一起转,它常含冒号)
  esc() { printf '%s' "$1" | tr '\r\n' '  ' | sed 's/%/%25/g; s/::/%3A%3A/g' | cut -c1-900; }
  printf '::error title=步骤失败::%s\n' "$(esc "退出码 $rc;命令:$*")"
  # 注解必须是单行:消息里带换行会把 GitHub 的工作命令格式截断(实际就这么丢掉了真正的错误行)
  grep -aE "^FAIL |^PARAM [A-Z]+: (FAIL|SKIP)|\.\.\. FAILED|test result: FAILED|^error(\[E[0-9]+\])?:" "$log" \
    | head -24 \
    | while IFS= read -r l; do
        printf '::error::%s\n' "$(esc "$l")"
      done
  # panic 的正文在 "panicked at" 的下一行,只抓一行等于没抓
  sed -n '/panicked at/,+2p' "$log" | head -30 | while IFS= read -r l; do
    printf '::error::%s\n' "$(esc "$l")"
  done
  # 最后几行兜底(权限、段错误这类没有固定形状的失败)
  tail -8 "$log" | while IFS= read -r l; do printf '::error::尾部 %s\n' "$(esc "$l")"; done
fi
exit $rc
