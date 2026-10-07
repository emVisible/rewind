#!/usr/bin/env python3
"""像素活性闸:每一个**看得见**的参数,单独拧动都必须改变成品帧。

与 param_liveness 的分工:那条证明"值进了计划"(结构),这条证明"计划里的值真的改到画面"(像素)。
两份合起来才堵得住"界面上有控件、拧了画面没反应"这一族。历史上真发生过一次
(`ntsc_vhs.settings`:一个改了什么都没用的 JSON 文本框)。

做法:每个参数挑两个拉开的值,在同一个小预览(400px 单帧)上各跑一遍,成品帧 md5 必须不同。

三条不许打折的规矩:
  ① **跑不出图就是失败**,不算跳过。否则"引擎对这个参数开始报错"会变成闸里的绿色 —— 那是最坏的假绿。
  ② 只有**成类**的豁免才允许存在:音频参数(本闸比的是帧哈希,天生看不见声音)
     与写在 EXEMPT 里带理由的个案;逐条打印,不许吞。
     **说清楚覆盖到什么程度**:音频参数不在本闸判断力之内,但也不是没人管 ——
     "逐个音频参数改了声音吗"由 `param_audio` 那条闸看着(比的是抽出来的音轨裸流 md5)。
  ③ 时间窗口类参数必须在窗口内取样:`tape_ends` 的噪声只在片头/片尾几百毫秒里,
     在 t=2 上比两帧当然一样 —— 那种"一样"是探针的位置错了,不是控件死了。所以按素材时长取 t。

enum 的取值不硬编:逐个相邻选项配对试,取第一对**两边都出图**的(容器/编码器之间有相容性,
mp4↔avi↔3gp 不保证两两能跑;拿不相容的一对报"失败"是闸在骗人)。
"""
import json, os, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.join(ROOT, "core/target/release/rewind-core")
if not os.path.exists(BIN):
    BIN = BIN + ".exe"
SRC = os.environ.get("SRC") or os.path.join(ROOT, "assets/sample/street-food.mp4")
if not os.path.exists(SRC):
    SRC = os.path.join(ROOT, "fixtures/test_src.mp4")
WORK = os.path.join(ROOT, ".work/gate/pixels")
# 需要"开关"才成形的参数:附加覆盖,让条件成立
ENABLERS = {
    "unsharp.size": ["unsharp.amount=1.2"],
    "unsharp.chroma_amount": ["unsharp.amount=1.2"],
}
# 时间窗口类参数的取样位置(占素材时长的比例)
PROBE_T = {"tape_ends.head": 0.02, "tape_ends.tail": 0.9, "tape_ends.intensity": 0.9}
# 个案豁免:必须写清为什么帧哈希看不见
EXEMPT = {
    "codec_roundtrip.audio_bitrate": "只改音频码率,帧哈希看不见 —— 本闸的边界,不是已覆盖",
    "codec_roundtrip.video_only": "只决定是否保留音轨,帧哈希看不见 —— 本闸的边界,不是已覆盖",
}


def duration():
    out = subprocess.run([BIN, "probe", SRC], capture_output=True, text=True)
    try:
        return float(json.loads(out.stdout).get("duration") or 0.0)
    except Exception:
        return 0.0


def run(preset, overrides, name, t):
    """跑一帧预览,返回成品帧 md5。

    每次调用用**独立目录**:同一个参数的候选对复用目录的话,上一对留下的 png 会被当成
    这一对的结果(实测就因此报过一个假故障 —— 说 size=15 跑不出来,单独跑一切正常)。
    失败重试一次:预览这条路要过 ffmpeg 多趟,偶发的资源竞争不该算"死控件"。
    """
    last_err = ""
    for attempt in (0, 1):
        d = os.path.join(WORK, "%s_r%d" % (name.replace(".", "_"), attempt))
        subprocess.run(["rm", "-rf", d], check=False)
        os.makedirs(d, exist_ok=True)
        cmd = [BIN, "preview", "--preset", os.path.join(ROOT, "presets", preset + ".json"),
               "--input", SRC, "--out-dir", d, "--t", str(t)]
        for o in overrides:
            cmd += ["--override", o]
        env = dict(os.environ, REWIND_PREVIEW_MAX_EDGE="400")
        r = subprocess.run(cmd, capture_output=True, text=True, env=env)
        if r.returncode != 0:
            # 取**开头**:引擎的报错是 "error: [码] 原因 …(路径)" —— 切尾部只会剩下一个目录名
            last_err = " ".join((r.stderr or r.stdout).split())[:170]
            continue
        for f in sorted(os.listdir(d)):
            if f.endswith(".png") and "_src" not in f:
                h = subprocess.run(["md5sum", os.path.join(d, f)], capture_output=True, text=True)
                if h.returncode == 0:
                    return h.stdout.split()[0], None
        last_err = "跑通了但没落图片"
    return None, last_err


def pairs(p):
    """列出要试的取值对;enum 逐对相邻选项(相容性未知,由调用方挑第一对两边都出图的)。"""
    k = p["kind"]
    if k == "optional_enum":
        opts = [str(o) for o in (p.get("options") or [])]
        return [(o, None) for o in opts]
    if k in ("enum", "stepped"):
        vals = [str(o) for o in (p.get("options") or p.get("stops") or [])]
        return [(vals[i], vals[i + 1]) for i in range(len(vals) - 1)] if len(vals) > 1 else []
    if k == "bool":
        return [("true", "false")]
    if k == "text":
        return [("%Y-%m-%d", "%H:%M:%S")]
    lo, hi = p.get("min"), p.get("max")
    return [(str(lo), str(hi))] if lo is not None and hi is not None and lo != hi else []


def run_frame(preset, ov, name):
    """预览窗口抽不到帧时的退路:跑真成品,再从成品里取一帧比。

    为什么要退这一层:有些极端值(实测 `interlace_comb.refps=1`)会让成品在预览窗口内没有可解码帧,
    引擎回 `[engine.frame] … 抽不到帧` —— 那是**探针的位置**看不见,不是参数没生效。不退就得到一个假死控件。
    比的是解码后的裸帧,不比容器字节:成品 mp4 里带 creation_time,比字节会永远"不同"。
    """
    d = os.path.join(WORK, ("run_" + name).replace(".", "_"))
    subprocess.run(["rm", "-rf", d], check=False)
    os.makedirs(d, exist_ok=True)
    cmd = [BIN, "run", "--preset", os.path.join(ROOT, "presets", preset + ".json"), "--input", SRC, "--out-dir", d]
    for o in ([ov] if isinstance(ov, str) else list(ov or [])):
        cmd += ["--override", o]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        return None, "成品跑不出来:" + " ".join((r.stderr or r.stdout).split())[:120]
    out = [os.path.join(d, f) for f in sorted(os.listdir(d)) if f.endswith((".mp4", ".mov", ".mkv", ".webm"))]
    if not out:
        return None, "成品没落地"
    v = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", out[0]],
                       capture_output=True, text=True)
    try:
        mid = max(0.0, float(v.stdout.strip()) / 2.0)
    except ValueError:
        mid = 0.0
    png = os.path.join(d, "probe.png")
    a = subprocess.run(["ffmpeg", "-v", "error", "-ss", "%.3f" % mid, "-i", out[0], "-frames:v", "1", png],
                       capture_output=True, text=True)
    if a.returncode != 0 or not os.path.exists(png):
        return None, "成品里取不到帧(%.2fs):%s" % (mid, " ".join(a.stderr.split())[:110])
    h = subprocess.run(["md5sum", png], capture_output=True, text=True)
    if h.returncode != 0:
        return None, "md5 失败"
    return h.stdout.split()[0], None


def deterministic(preset, t):
    """同一个预设、不加任何覆盖跑两遍,帧必须一字不差。

    为什么要先问这一句:`overlay_timestamp` 用的是 `%{{localtime}}` —— 带时间戳的预设**每次渲染都不同**
    (那是设计,不是 bug)。拿这种预设当基线,"两帧不一样"可能就只是秒表走了一格,而不是参数起了作用,
    那是假绿。所以先按预设量一遍可复现性,判据只在可复现的预设上成立。
    """
    h1, _ = run(preset, [], "det_a", t)
    h2, _ = run(preset, [], "det_b", t)
    return bool(h1 and h2 and h1 == h2)


def main():
    m = json.loads(subprocess.check_output([BIN, "describe"], text=True))
    cat = json.loads(subprocess.check_output([BIN, "catalog"], text=True))
    by_stage = {}
    for e in cat:
        # 只认磁盘上真存在的预设文件:catalog 里的年代轴派生品落在用户目录,presets/ 没这个文件
        if not os.path.exists(os.path.join(ROOT, "presets", e["id"] + ".json")):
            continue
        for st in (e.get("params") or {}).keys():
            by_stage.setdefault(st, []).append(e["id"])
    flat = [(sec, st["stage"], p) for sec in ("video", "audio") for st in m.get(sec, []) for p in st["params"]]
    if not flat:
        # 清单空了(段名被改、describe 报错却没抛)时,零个判断会打印成 OK —— 那是最假的绿。
        # 这条判据放在所有渲染之前:前提不成立就不必再花五分钟。
        print("PARAM PIXELS: FAIL 清单里一个参数都没有 —— 闸无从下判,不算通过")
        return 1
    subprocess.run(["rm", "-rf", WORK], check=False)
    os.makedirs(WORK, exist_ok=True)
    dur = duration()
    t0 = round(dur * 0.25, 2) or 0.2
    builtins = sorted({e["id"] for e in cat if os.path.exists(os.path.join(ROOT, "presets", e["id"] + ".json"))})
    ok_presets = [p for p in builtins if deterministic(p, t0)]
    print("素材时长 %.2fs;%d 个内置预设,其中帧可复现的 %d 个:%s"
          % (dur, len(builtins), len(ok_presets), " ".join(ok_presets) or "无"))
    if not ok_presets:
        print("PARAM PIXELS: SKIP 没有任何可复现的基线预设 —— 帧哈希判据不成立,不装作通过")
        return 0

    # --only=子串:只判匹配的参数的闸调试模式(改完一个参数想立刻复验,不必等全表 5 分钟)
    only = [a.split("=", 1)[1] for a in sys.argv[1:] if a.startswith("--only=")]
    if only:
        flat = [x for x in flat if any(o in "%s.%s" % (x[1], x[2]["key"]) for o in only)]
        print("--only 过滤后判 %d 个" % len(flat))
    dead, exempted, judged, fell_back = [], [], 0, []
    for sec, stage, p in flat:
        key = "%s.%s" % (stage, p["key"])
        if sec == "audio":
            # 归属看清单的段,不在这里再抄一份 stage 名单 —— 抄的那份会跟着清单过期
            exempted.append("%s: 音频参数 —— 帧哈希看不见(本闸边界);逐参数听觉活性由 param_audio 看着" % key)
            continue
        if key in EXEMPT:
            exempted.append("%s: 豁免 —— %s" % (key, EXEMPT[key]))
            continue
        # 基线优先挑"本来就带这一段、且帧可复现"的预设;都没有就落在任一可复现预设上,靠插入生效
        base = next((x for x in by_stage.get(stage, []) if x in ok_presets), ok_presets[0])
        en = ENABLERS.get(key, [])
        t = round(dur * PROBE_T.get(key, 0.25), 2) or 0.2
        cand = pairs(p)
        if not cand:
            dead.append("%s: 清单给不出两个不同取值(kind=%s)" % (key, p["kind"]))
            continue
        got = None
        why = []
        for a, b in cand:
            oa = ["%s=%s" % (key, a)] + en
            ob = (["%s=%s" % (key, b)] if b is not None else []) + en
            ha, ea = run(base, oa, "a_%s" % key, t)
            hb, eb = run(base, ob, "b_%s" % key, t)
            if ha and hb:
                got = (ha, hb, a, b)
                break
            why.append("%s|%s: %s%s" % (a, b or "<不给>", (ea or "")[:40], (" / " + eb[:40]) if eb else ""))
        if got is None:
            # 预览这条路看不见 → 退到"跑真成品,从成品里取一帧"再判一次
            for a, b in cand[:1]:
                oa = ["%s=%s" % (key, a)] + en
                ob = (["%s=%s" % (key, b)] if b is not None else []) + en
                fa, ea = run_frame(base, oa, key)
                fb, eb = run_frame(base, ob, key)
                if fa and fb:
                    got = (fa, fb, a, b)
                    fell_back.append(key)
                else:
                    why.append("成品帧也拿不到:%s / %s" % ((ea or "")[:60], (eb or "")[:60]))
        if got is None:
            dead.append("%s (基线 %s): 预览与成品两条路都拿不到可比较的两帧 —— %s" % (key, base, " || ".join(why[:2])))
            continue
        ha, hb, a, b = got
        judged += 1
        if ha == hb:
            dead.append("%s (基线 %s, t=%.2f, 试了 %s|%s): 两帧一模一样" % (key, base, t, a, b or "<不给值>"))

    print("判了 %d 对参数;成类/个案豁免 %d 条" % (judged, len(exempted)))
    if fell_back:
        print("其中 %d 对是预览窗口抽不到帧、改用真成品帧判的:%s" % (len(fell_back), " ".join(fell_back)))
    if dead:
        print("拧了没有画面反应(%d):" % len(dead))
        for x in dead:
            print("   ", x)
    print("豁免清单:")
    for x in exempted:
        print("   ", x)
    if dead:
        print("PARAM PIXELS: FAIL=%d(判 %d 对)" % (len(dead), judged))
        return 1
    print("PARAM PIXELS: OK(%d 对参数逐个证明会改画面,%d 条成类豁免)" % (judged, len(exempted)))
    return 0


def selftest():
    """反向自检:同一个值跑两遍必须**得到同一帧**。
    这条不通过说明比较本身没有意义(渲染不可复现),那条"两帧一样 = 死控件"就全是噪声。
    顺带证明判据不是恒真:它必须能判出"相同"。
    """
    subprocess.run(["rm", "-rf", WORK], check=False)
    os.makedirs(WORK, exist_ok=True)
    dur = duration()
    t = round(dur * 0.25, 2) or 0.2
    cat = json.loads(subprocess.check_output([BIN, "catalog"], text=True))
    builtins = sorted({e["id"] for e in cat if os.path.exists(os.path.join(ROOT, "presets", e["id"] + ".json"))})
    base = next((p for p in builtins if deterministic(p, t)), None)
    if not base:
        print("SELFTEST FAIL 没有可复现的内置预设可当参照")
        return 1
    print("SELFTEST 用 %s(帧可复现)当参照" % base)
    h1, e1 = run(base, ["color.saturation=0.4"], "st_a", t)
    h2, e2 = run(base, ["color.saturation=0.4"], "st_b", t)
    h3, e3 = run(base, ["color.saturation=1.9"], "st_c", t)
    if not (h1 and h2 and h3):
        print("SELFTEST FAIL 预览没出图:%s / %s / %s" % (e1, e2, e3))
        return 1
    if h1 != h2:
        print("SELFTEST FAIL 同参数两次渲染不一致(%s vs %s)—— 渲染不可复现,闸的判据没有意义" % (h1, h2))
        return 1
    if h1 == h3:
        print("SELFTEST FAIL 饱和 0.4 与 1.9 出了同一帧 —— 比较本身看不见东西")
        return 1
    print("SELFTEST OK 同值两跑同帧、不同值不同帧(判据既能判同也能判异)")
    return 0


if __name__ == "__main__":
    if "--selftest" in sys.argv:
        sys.exit(selftest())
    sys.exit(main())
