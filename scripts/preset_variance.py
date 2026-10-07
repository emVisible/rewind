#!/usr/bin/env python3
"""预设差异度闸的实现体(由 preset_variance.sh 调用)。

为什么不用全局缩略图的平均色:实测 crt1995 与 vhs1990_static 的 YUV 最大差只有 1.67,
64×64 缩略把扫描线与宏块都平均掉了 —— 平均色看不见纹理。所以这里三信号取**或**:
  色距(整帧缩略) / 高频能量(1:1 大图的右半) / 8 像素周期块度(同一右半)
阈值来自 2026-10-05 对 12 个预设的实测:65 对里最小综合距离 1.48,判据取 1.0。
"""
import itertools
import json
import math
import os
import subprocess
import sys

MIN_COLOR = float(os.environ.get("REWIND_MIN_COLOR", "6.0"))   # max(|ΔY|,|ΔU|,|ΔV|)
MIN_HF = float(os.environ.get("REWIND_MIN_HF", "0.15"))        # 拉普拉斯标准差的相对差
MIN_BLOCK = float(os.environ.get("REWIND_MIN_BLOCK", "0.12"))  # 8px 周期二阶差的相对差
# 色度离散度(整帧 U/V 标准差)的相对差:胶片褪色把色度压向灰,数码包浆保留色度但带断层,
# 这一条专门分这两种"均值看起来一样"的情况(实测 film1970 11.21 vs patina 16.53 → 0.32)。
MIN_CHROMA = float(os.environ.get("REWIND_MIN_CHROMA", "0.20"))
GALLERY = "app/ui/gallery"


def gray_plane(png):
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v", "-show_entries",
         "stream=width,height", "-of", "csv=p=0", png],
        capture_output=True, text=True).stdout
    w, h = (int(x) for x in out.strip().split(",")[:2])
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", png, "-vf", "format=gray", "-f", "rawvideo", "-"],
        capture_output=True).stdout
    return list(raw[:w * h]), w, h


def texture(png):
    """1:1 大图右半(跳过中间分隔线)的高频能量与块度。"""
    px, w, h = gray_plane(png)
    x0 = w // 2 + 4
    s = s2 = 0.0
    n = 0
    bs = 0.0
    bn = 0
    for y in range(1, h - 1):
        base = y * w
        for x in range(x0 + 1, w - 1):
            i = base + x
            v = 4 * px[i] - px[i - 1] - px[i + 1] - px[i - w] - px[i + w]
            s += v
            s2 += v * v
            n += 1
            bs += abs(2 * px[i] - px[i - 8] - px[i + 8])
            bn += 1
    if not n:
        return None
    mean = s / n
    return math.sqrt(max(s2 / n - mean * mean, 0.0)), bs / max(bn, 1)


def color_avg(png):
    out = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", png, "-vf",
         "signalstats,metadata=print:file=-", "-f", "null", "-"],
        capture_output=True, text=True).stdout
    d = {}
    for line in out.splitlines():
        for k in ("YAVG", "UAVG", "VAVG"):
            tag = "lavfi.signalstats.%s=" % k
            if tag in line:
                d[k] = float(line.split(tag, 1)[1].split()[0])
    if len(d) != 3:
        return None
    return (d["YAVG"], d["UAVG"], d["VAVG"])


def chroma_spread(png):
    """整帧 U/V 标准差的均值:褪色(压向灰)与数码压缩(保留色度但断层)在这条上分道。"""
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v", "-show_entries",
         "stream=width,height", "-of", "csv=p=0", png],
        capture_output=True, text=True).stdout
    w, h = (int(x) for x in out.strip().split(",")[:2])
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", png, "-vf", "format=yuv420p", "-f", "rawvideo", "-"],
        capture_output=True).stdout
    n = w * h
    cw, ch = w // 2, h // 2
    sd = []
    for off in (n, n + cw * ch):
        px = raw[off:off + cw * ch]
        if len(px) < cw * ch:
            return None
        mean = sum(px) / len(px)
        sd.append((sum((x - mean) ** 2 for x in px) / len(px)) ** 0.5)
    return sum(sd) / len(sd)


def fingerprint(pid):
    """一个预设的结构指纹:stage 集合 + 全部参数值(浅/重两档只差参数也要算不同)。"""
    with open("presets/%s.json" % pid, encoding="utf-8") as f:
        d = json.load(f)
    stages = sorted(s["stage"] for s in d.get("video", []))
    params = json.dumps({s["stage"]: s.get("params", {}) for s in d.get("video", [])},
                        sort_keys=True)
    return (tuple(stages), params, d.get("aging"))


def main():
    whitelist = sys.argv[1] if len(sys.argv) > 1 else ""
    ids = sorted(f[:-5] for f in os.listdir("presets") if f.endswith(".json"))
    fails = 0

    print("=== 1. 结构:预设不得完全雷同 ===")
    dup = 0
    for a, b in itertools.combinations(ids, 2):
        if fingerprint(a) == fingerprint(b):
            print("FAIL  %s 与 %s 的 stage 集合与参数完全相同" % (a, b))
            dup += 1
    if not dup:
        print("PASS  %d 个预设互不重复(stage 集合 + 参数 + 手数)" % len(ids))
    fails += dup

    print("=== 2. 观感:色距 / 高频 / 块度 / 色度离散度 四信号取或 ===")
    fp = {}
    for i in ids:
        full, hero = "%s/%s_full.png" % (GALLERY, i), "%s/%s_hero.webp" % (GALLERY, i)
        missing = [p for p in (full, hero) if not os.path.isfile(p)]
        if missing:
            print("FAIL  %s 缺资产 %s(跑 bash scripts/preset_gallery.sh)" % (i, " ".join(missing)))
            fails += 1
            continue
        c, t, cs = color_avg(full), texture(hero), chroma_spread(full)
        if c is None or t is None or cs is None:
            print("FAIL  %s 资产读不出指标" % i)
            fails += 1
            continue
        fp[i] = (c, t, cs)

    worst, worst_pair = None, ("", "")
    checked = 0
    for (a, (ca, ta, sa)), (b, (cb, tb, sb)) in itertools.combinations(sorted(fp.items()), 2):
        if "%s|%s" % (a, b) == whitelist or "%s|%s" % (b, a) == whitelist:
            continue
        checked += 1
        cd = max(abs(x - y) for x, y in zip(ca, cb))
        dh = abs(ta[0] - tb[0]) / max(ta[0], tb[0], 1e-9)
        db = abs(ta[1] - tb[1]) / max(ta[1], tb[1], 1e-9)
        ds = abs(sa - sb) / max(sa, sb, 1e-9)
        score = max(cd / MIN_COLOR, dh / MIN_HF, db / MIN_BLOCK, ds / MIN_CHROMA)
        if score < 1.0:
            print("FAIL  %s 与 %s 看着一样:色距 %.2f / HF差 %.3f / 块度差 %.3f / 色度离散差 %.3f(综合 %.2f < 1)"
                  % (a, b, cd, dh, db, ds, score))
            fails += 1
        elif worst is None or score < worst:
            worst, worst_pair = score, (a, b)
    if checked:
        print("PASS  %d 对预设全部可分辨;最接近的一对是 %s vs %s,综合距离 %.2f(阈值 1.0)"
              % (checked, worst_pair[0], worst_pair[1], worst))
    fails += len(ids) - len(fp)

    print()
    print("VARIANCE: %s" % ("OK" if fails == 0 else "FAIL=%d" % fails))
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
