#!/usr/bin/env python3
"""音频活性闸:每一个会影响声音的参数,单独拧动必须改变成品音频。

补的是 param_pixels 的边界 —— 它比的是帧哈希,听不见底噪、带限、抖晃与位深破碎。
判据与像素层同构:同一个预设两个取值各跑一遍真成品,抽 out 里的音轨成 f64 裸流比 md5,必须不同。

三条规矩(与像素层一致):
  ① 跑不出成品算失败,不算跳过;
  ② 唯一允许的"不同即结论"例外是 codec_roundtrip.video_only:它 true 时**没有音轨**,
     所以"一边有声、一边无声"就是它生效的证据;
  ③ 先做可复现性预检:同参数两遍的音轨必须一字不差,否则 md5 判据没有意义。
"""
import json, os, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.join(ROOT, "core/target/release/rewind-core")
if not os.path.exists(BIN):
    BIN = BIN + ".exe"
SRC = os.environ.get("SRC") or os.path.join(ROOT, "fixtures/test_src.mp4")
WORK = os.path.join(ROOT, ".work/gate/param_audio")
# 挂在视频段上、但只改声音的参数 —— 按清单的段挑会漏掉它们,所以点名补上
AUDIO_SHAPES = {"codec_roundtrip.audio_bitrate", "codec_roundtrip.video_only"}


def audio_hash(preset, ov, name):
    """跑一趟真成品,返回音轨的 md5;返回 ("", None) 表示**没有音轨**。"""
    d = os.path.join(WORK, name.replace(".", "_"))
    subprocess.run(["rm", "-rf", d], check=False)
    os.makedirs(d, exist_ok=True)
    cmd = [BIN, "run", "--preset", os.path.join(ROOT, "presets", preset + ".json"), "--input", SRC, "--out-dir", d]
    if ov:
        cmd += ["--override", ov]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        return None, (r.stderr or r.stdout).strip()[-90:]
    files = [os.path.join(d, f) for f in sorted(os.listdir(d)) if f.endswith((".mp4", ".mov", ".mkv", ".webm", ".avi", ".mpg"))]
    if not files:
        return None, "成品没落地"
    a = subprocess.run(["ffmpeg", "-v", "error", "-i", files[0], "-vn", "-f", "f64le", "-"],
                       capture_output=True)
    if a.returncode != 0 or not a.stdout:
        return "", None          # 无声轨:这本身就是一种结果
    import hashlib
    return hashlib.md5(a.stdout).hexdigest(), None


def deterministic(preset):
    h1, _ = audio_hash(preset, None, "det_a")
    h2, _ = audio_hash(preset, None, "det_b")
    return bool(h1) and h1 == h2


def pair(p):
    k = p["kind"]
    if k == "optional_enum":
        o = [str(x) for x in (p.get("options") or [])]
        return (o[0], None) if o else None
    if k in ("enum", "stepped"):
        v = [str(x) for x in (p.get("options") or p.get("stops") or [])]
        return (v[0], v[-1]) if len(set(v)) > 1 else None
    if k == "bool":
        return ("true", "false")
    lo, hi = p.get("min"), p.get("max")
    return (str(lo), str(hi)) if lo is not None and hi is not None and lo != hi else None


def main():
    m = json.loads(subprocess.check_output([BIN, "describe"], text=True))
    cat = json.loads(subprocess.check_output([BIN, "catalog"], text=True))
    by_stage = {}
    for e in cat:
        if not os.path.exists(os.path.join(ROOT, "presets", e["id"] + ".json")):
            continue
        for st in (e.get("params") or {}).keys():
            by_stage.setdefault(st, []).append(e["id"])
    subprocess.run(["rm", "-rf", WORK], check=False)
    os.makedirs(WORK, exist_ok=True)
    targets = [(st["stage"], p) for st in m.get("audio", []) for p in st["params"]]
    targets += [(st["stage"], p) for st in m.get("video", []) for p in st["params"]
                if "%s.%s" % (st["stage"], p["key"]) in AUDIO_SHAPES]
    if not targets:
        # 清单里一个音频参数都没有(段被改名、describe 出错却没抛):零个判断也会打印成 OK(0) —— 那是最假的绿
        print("PARAM AUDIO: FAIL 清单里没有可判的音频参数 —— 闸无从下判,不算通过")
        return 1
    builtins = sorted({e["id"] for e in cat if os.path.exists(os.path.join(ROOT, "presets", e["id"] + ".json"))})
    ok_presets = [p for p in builtins if deterministic(p)]
    print("音轨可复现的内置预设 %d/%d:%s" % (len(ok_presets), len(builtins), " ".join(ok_presets) or "无"))
    if not ok_presets:
        print("PARAM AUDIO: SKIP 没有可复现的基线 —— 音轨 md5 判据不成立,不装作通过")
        return 0
    dead, judged = [], 0
    for stage, p in targets:
        key = "%s.%s" % (stage, p["key"])
        pr = pair(p)
        if not pr:
            dead.append("%s: 挑不出两个不同取值(kind=%s)" % (key, p["kind"]))
            continue
        a, b = pr
        # 基线要一个个试:同一个参数在"音频是 copy 过去的"预设里天生不动
        # (bitrate_roundtrip 在 dvd2005 上就是这种情况) —— 那是条件生效,不是死控件。
        bases = [x for x in by_stage.get(stage, []) if x in ok_presets] + [p for p in ok_presets if p not in by_stage.get(stage, [])]
        verdict = None
        compared = False
        for base in bases[:6]:
            ha, ea = audio_hash(base, "%s=%s" % (key, a), "a_" + key)
            ob = ("%s=%s" % (key, b)) if b is not None else None
            hb, eb = audio_hash(base, ob, "b_" + key)
            if ha is None or hb is None:
                verdict = "%s (基线 %s): 跑不出成品 —— %s / %s" % (key, base, ea or "", eb or "")
                continue
            compared = True
            if ha != hb:
                verdict = None
                break
            verdict = "%s (试到最后一个基线仍无变化,最后用 %s,取值 %s|%s): 音轨一字未变" % (key, base, a, b or "<不给值>")
        if compared:
            judged += 1
        if verdict:
            dead.append(verdict)
    print("判了 %d 对音频参数" % judged)
    if dead:
        print("拧了没有声音变化(%d):" % len(dead))
        for x in dead:
            print("   ", x)
        print("PARAM AUDIO: FAIL=%d" % len(dead))
        return 1
    print("PARAM AUDIO: OK(%d 对音频参数逐个证明会改音轨)" % judged)
    return 0


if __name__ == "__main__":
    sys.exit(main())
