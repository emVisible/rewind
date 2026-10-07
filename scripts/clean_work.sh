#!/usr/bin/env bash
# .work/ 治理入口:报告每个子目录的体积、谁生成它、删了怎么回来。
# 默认只报告不删 —— 这个目录里有 291 MB 是历史里程碑的证据图,`重构文档.md` 按文件名引用它们,
# 一把 rm -rf 会把台账里的引用变成死链。所以删除要显式选类别。
# 用法: bash scripts/clean_work.sh              # 报告
#       bash scripts/clean_work.sh --gate      # 只删各闸输出(重跑各闸即回)
#       bash scripts/clean_work.sh --targets   # cargo clean 两个 crate(target 胖是没人清,不是结构问题)
#       bash scripts/clean_work.sh --evidence  # 连证据图一起删(不可再生,需 --yes 二次确认)
set -u
cd "$(dirname "$0")/.."
W=.work
[ -d "$W" ] || { echo "没有 $W/ —— 一切都是干净的"; exit 0; }

report() {
  printf '%-22s %8s %6s  %s\n' "目录" "体积" "文件" "来源与回归方式"
  for d in "$W"/*/; do
    [ -d "$d" ] || continue
    name=$(basename "$d")
    size=$(du -sh "$d" 2>/dev/null | cut -f1)
    n=$(find "$d" -type f | wc -l)
    case "$name" in
      gate)    note="各闸输出(regression/aging/conc/geo/axis/release/三条活性闸的中间图与音轨)—— 重跑各闸即回" ;;
      samples) note="里程碑证据图与定标中间片 —— **多数不可再生**,台账按文件名引用" ;;
      *)       note="未登记的目录 —— 要么改本脚本,要么删掉它" ;;
    esac
    printf '%-22s %8s %6s  %s\n' "$name" "$size" "$n" "$note"
  done
  echo
  echo "合计 $(du -sh "$W" | cut -f1)"
  # 未登记目录本身就是问题:静默长出来的东西没人知道能不能删
  EXTRA=$(find "$W" -maxdepth 1 -type d -mindepth 1 ! -name gate ! -name samples ! -name "$W" | wc -l)
  [ "$EXTRA" = "0" ] || echo "⚠ 有 $EXTRA 个未登记子目录"
}

case "${1:-}" in
  --gate)
    rm -rf "$W/gate" && echo "已删 $W/gate(重跑各闸即回)"; report ;;
  --targets)
    # 实测:core/shell 各自干净 release 构建 = 131M + 39M,而增量堆积能长到 833M + 213M。
    # 也就是说 target 的"胖"不是结构问题,是从来没清过 —— 一条命令的事。
    before=$(du -shc core/target shell/target 2>/dev/null | tail -1 | cut -f1)
    (cd core && cargo clean); (cd shell && cargo clean)
    echo "cargo clean 完成:清理前 $before → 现在 $(du -shc core/target shell/target 2>/dev/null | tail -1 | cut -f1 || echo '0(目录已没了)')"
    echo "重新构建实测约 19 s(core 15 + shell 4)" ;;
  --evidence)
    if [ "${2:-}" != "--yes" ]; then
      echo "拒绝:$W/samples 里的证据图不可再生(重构文档按文件名引用)。确认要删就加 --yes"; exit 2
    fi
    rm -rf "$W/samples" && echo "已删 $W/samples"; report ;;
  *) report ;;
esac
