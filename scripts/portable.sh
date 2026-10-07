#!/usr/bin/env bash
# 闸脚本要在三端跑,而 GNU coreutils 与 BSD 工具在这三件事上不通用:
#   * `md5sum` 是 GNU 的(BSD/macOS 只有 `md5 -q`)
#   * `stat -c%s` 是 GNU 的(BSD 是 `stat -f%z`)
#   * `find -printf` 是 GNU 的(BSD find 没这个选项,**输出直接变空**)
# 这三种写法在 Linux 上跑得好,在 macOS 上会让判据读到空值 —— 于是"画面没变"或
# "输出过小/缺失"报的是**判据自己**故障。CI 的 macOS 作业就是这三条的验尸报告。
# 用法:`. "$(dirname "$0")/portable.sh"`,然后一律用 fsize / md5of / md5pipe。

fsize() { # 文件字节数;文件不存在输出 0,永不返回失败(判据自己决定怎么算缺件)
  if [ -f "$1" ]; then wc -c <"$1" | tr -d ' '; else printf '0\n'; fi
}

md5of() { # 文件的 md5(取不到就输出空串)
  [ -f "$1" ] || return 0
  if command -v md5sum >/dev/null 2>&1; then
    md5sum "$1" | cut -d' ' -f1
  elif command -v md5 >/dev/null 2>&1; then
    md5 -q "$1"
  fi
}

md5pipe() { # 标准输入的 md5( ffmpeg 直接吐流时用这个,不落临时文件 )
  if command -v md5sum >/dev/null 2>&1; then
    md5sum | cut -d' ' -f1
  elif command -v md5 >/dev/null 2>&1; then
    md5 -q
  fi
}
