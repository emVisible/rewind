#!/usr/bin/env python3
"""参数活性闸:清单里每个参数都必须在计划里留下可见痕迹。

为什么需要这条闸:死控件在界面上长得跟真控件一模一样。历史上真发生过两次 ——
`ntsc_vhs.settings`(JSON 文本框,值传到引擎被丢弃)与 `bitcrush.mode` 的选项表与 ffmpeg 不匹配;
单测、回归闸、画廊都不红,只有用户"拧了没反应"。

判据(结构层,不渲像素):存在一个上下文(预设 [+ 附加覆盖]),使得
`--override stage.key=A` 与 `--override stage.key=B` 两份 `rewind-core plan` 输出**不同**。
全都相同 = 这个值既没进滤镜链、也没进任何 stage 结构 = 嫌疑死控件。

为什么要有"附加覆盖"的上下文:有的参数天生被另一个参数关着 —— `unsharp.size` 只在
`amount > 0` 时才生成滤镜,而爆炸档是做旧系数 ≥6 手才进来的(实测)。这不是死控件,是条件生效,
所以要在 `preset.aging=7` 的上下文里再试一遍。

optional_enum 只有"给值 / 不给值"两种状态(如 `fps.cadence` 只发布 telecine32),按这个形状配对。

真正扫不出差别的参数必须写进 EXEMPT 并**给出理由**,空表才算这条闸有意义。
"""
import json, os, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.environ.get("REWIND_BIN") or os.path.join(ROOT, "core/target/release/rewind-core")
if not os.path.exists(BIN):
    BIN = BIN + ".exe"
SRC = os.environ.get("SRC") or os.path.join(ROOT, "assets/sample/street-food.mp4")
if not os.path.exists(SRC):
    SRC = os.path.join(ROOT, "fixtures/test_src.mp4")
PRESETS = os.path.join(ROOT, "presets")

# (预设, 附加覆盖) —— 覆盖面广的排前面,能少跑几十次 plan
CONTEXTS = [
    ("patina", ["unsharp.amount=1.2"]),     # unsharp 的 size/chroma_amount 只有 amount>0 才成形
    ("patina", ["preset.aging=7"]),
    ("patina", []),
    ("vhs1990_ntscrs", []),
    ("cctv2000", []),
    ("dvd2005", []),
    ("film1970", []),
    ("crt1995", []),
    ("phone2010", []),
    ("rmvb2006", []),
    ("blurhigh", []),
    ("screenbat", []),
]

# 扫不出差别的参数:写清为什么可以没有痕迹(带理由才允许存在)
EXEMPT = {}


def plan(preset, extra, ov):
    cmd = [BIN, "plan", "--preset", os.path.join(PRESETS, preset + ".json"), "--input", SRC]
    for e in extra:
        cmd += ["--override", e]
    if ov:
        cmd += ["--override", ov]
    r = subprocess.run(cmd, capture_output=True, text=True)
    return r.returncode, r.stdout


def pair(p):
    """挑两个必然不同的取值;第二个可以是 None,表示"不给这个参数"(optional_enum 的两个状态)。"""
    k = p["kind"]
    if k == "optional_enum":
        opts = p.get("options") or []
        return (str(opts[0]), None) if opts else None
    if k in ("enum", "stepped"):
        vals = [str(o) for o in (p.get("options") or p.get("stops") or [])]
        return (vals[0], vals[-1]) if len(set(vals)) > 1 else None
    if k == "bool":
        return ("true", "false")
    if k == "text":
        return ("aaa", "bbb")
    lo, hi = p.get("min"), p.get("max")
    if lo is None or hi is None or lo == hi:
        return None
    return (str(lo), str(hi))


def structure(m):
    """清单内部必须自洽:控件绑到不存在的参数 = 界面上少一个控件而且没人报错。

    这几条不花 plan,所以顺手一起判 —— 同一族事故(ntsc_vhs.settings)的形状就是
    "界面有东西、引擎没人吃",只不过发生在控件层而不是参数层。
    """
    bad = []
    have = {(st["stage"], p["key"]) for sec in ("video", "audio") for st in m.get(sec, []) for p in st["params"]}
    stages = {st["stage"] for sec in ("video", "audio") for st in m.get(sec, [])}
    gids = {g["id"] for g in m.get("groups", [])}
    used = {st.get("group") for sec in ("video", "audio") for st in m.get(sec, [])}
    for g in sorted(used - gids):
        bad.append("分组 %s 被 stage 用了却没在 groups 里声明(高级面板会直接显示原始 id)" % g)
    for g in sorted(gids - used):
        bad.append("分组 %s 声明了却没有 stage 属于它(清单在漂移)" % g)
    seen = {}
    for c in m.get("controls", []):
        if not c.get("binds"):
            bad.append("一级控件组 %s 没有绑定任何参数(整组是死的)" % c["id"])
        for b in c.get("binds", []):
            k = (b["stage"], b["key"])
            if b["stage"] not in stages:
                bad.append("%s 绑到不存在的 stage %s" % (c["id"], b["stage"]))
            elif k not in have:
                bad.append("%s 绑到清单里没有的参数 %s.%s" % (c["id"], b["stage"], b["key"]))
            if k in seen:
                bad.append("%s.%s 同时被两个一级控件组绑定(%s / %s)—— 一个参数只许一个控件"
                           % (k[0], k[1], seen[k], c["id"]))
            seen[k] = c["id"]
    return bad, len(seen)


def main():
    m = json.loads(subprocess.check_output([BIN, "describe"], text=True))
    flat = [(st["stage"], p) for sec in ("video", "audio") for st in m.get(sec, []) for p in st["params"]]
    if not flat:
        # 清单空了(段被改名、describe 出错却没抛):"0 个参数全部留痕"也是一句真话,但它什么都没说
        print("PARAM LIVENESS: FAIL 清单里一个参数都没有 —— 闸无从下判,不算通过")
        return 1
    structural, nbinds = structure(m)
    for s in structural:
        print("结构问题:", s)
    dead, skipped, tries = [], [], 0
    for stage, p in flat:
        key = "%s.%s" % (stage, p["key"])
        pr = pair(p)
        if not pr:
            skipped.append("%s: 清单给不出两个不同取值(kind=%s)" % (key, p["kind"]))
            continue
        a, b = pr
        hit = False
        for preset, extra in CONTEXTS:
            oa = "%s=%s" % (key, a)
            ob = "%s=%s" % (key, b) if b is not None else None
            ra, rb = plan(preset, extra, oa), plan(preset, extra, ob)
            if ra[0] != 0 or rb[0] != 0:
                continue
            tries += 2
            if ra[1] != rb[1]:
                hit = True
                break
        if not hit:
            (skipped if key in EXEMPT else dead).append(
                "%s (试了 %s|%s,共 %d 个上下文)" % (key, a, b or "<不给值>", len(CONTEXTS))
                + (" —— 豁免理由:%s" % EXEMPT[key] if key in EXEMPT else ""))
    print("参数 %d 个;一级绑定 %d 条;plan 对比跑了 %d 次" % (len(flat), nbinds, tries))
    for label, items in (("死控件嫌疑", dead), ("未判定/豁免", skipped)):
        if items:
            print("%s(%d):" % (label, len(items)))
            for x in items:
                print("   ", x)
    if dead or structural:
        print("PARAM LIVENESS: FAIL=%d(死控件 %d + 结构 %d)" % (len(dead) + len(structural), len(dead), len(structural)))
        return 1
    print("PARAM LIVENESS: OK(%d 个参数全部能在计划里留痕)" % len(flat))
    return 0


if __name__ == "__main__":
    sys.exit(main())
