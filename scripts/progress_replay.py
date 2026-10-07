#!/usr/bin/env python3
"""进度策略选形:拿实测的逐趟耗时回放三种"总体百分比",看谁离真实时间比例最近。

真实读数 = 已用墙钟 / 任务总墙钟。策略只能看到当时已知的信息:
趟号、趟数、每趟 basis(画布×帧率)、本趟引擎自己报的百分比、已用墙钟。

  S1 等分         :每趟 1/n(现状)
  S2 静态成本模型 :成本 = F + rate(路径) × 像素-帧,常数拟合本数据
  S3 吞吐自适应   :用当前趟实测吞吐外推其余趟(已跑完的趟用实测)
  S4 混合         :S3,但没跑过的**异路径**趟按静态比值给先验

判据:总体读数与真实比例的 MAE / 最坏偏差(百分点)。
"""
import json
from collections import defaultdict
from math import fabs

rows = defaultdict(list)
for l in open(".work/step_cost.jsonl"):
    r = json.loads(l)
    rows[(r["preset"], r["clip"])].append(r)
jobs = [v for v in rows.values() if sum(x["secs"] for x in v) > 0.4]
print(f"参与回放的工作数 {len(jobs)}(趟数合计 {sum(len(j) for j in jobs)})")
SAMPLES = 200


def replay(job, mode, F=0.0, Rp=0.01, Rf=0.003, clamp=True, prior=3.0):
    """采样按**墙钟等间隔**(人眼看到的就是墙钟上的读数),不是按趟等分。"""
    total = sum(r["secs"] for r in job)
    if mode == "static":
        w = [F + (Rp if r["kind"] == "pixel" else Rf) * r["pxf"] for r in job]
    else:
        w = [1.0] * len(job)
    tw = sum(w)
    # 每趟的时间边界(墙钟)与权重边界
    tb, acc = [], 0.0
    for r in job:
        tb.append((acc, acc + r["secs"]))
        acc += r["secs"]
    disp, errs = 0.0, []
    for k in range(1, SAMPLES + 1):
        el = total * k / SAMPLES
        i = next(x for x, (a, b) in enumerate(tb) if el <= b or x == len(job) - 1)
        a, b = tb[i]
        frac = max(0.0, min(1.0, (el - a) / max(b - a, 1e-9)))
        true = el / total * 100.0
        if mode == "static":
            v = (sum(w[:i]) + w[i] * frac) / tw * 100.0
        else:
            r = job[i]
            # 引擎的 pct 是"本趟输出时间/本趟总时间",所以本趟总量 = 已用/pct
            est_self = r["secs"] * frac / max(frac, 1e-6)
            rate = est_self / max(r["pxf"], 1e-6)   # 秒 / 百万像素-帧
            est = []
            for j, x in enumerate(job):
                if j < i:
                    est.append(x["secs"])
                elif j == i:
                    est.append(est_self)
                elif x["kind"] == r["kind"]:
                    est.append(rate * x["pxf"])
                else:
                    q = rate * (prior if x["kind"] == "pixel" else 1.0 / prior)
                    est.append(q * x["pxf"])
            v = el / max(sum(est), 1e-6) * 100.0
        if clamp:
            v = max(v, disp)
        disp = v
        errs.append(fabs(v - true))
    return sum(errs) / len(errs), max(errs)


def score(fn):
    ms, worst = [], 0.0
    for job in jobs:
        m, w = fn(job)
        ms.append(m)
        worst = max(worst, w)
    ms.sort()
    return sum(ms) / len(ms), ms[len(ms) // 2], worst


def show(name, fn):
    a, md, w = score(fn)
    print(f"  {name:<24} 平均 MAE {a:5.1f}pp  中位 {md:5.1f}pp  最坏单点 {w:5.1f}pp")


show("S1 等分", lambda j: replay(j, "equal"))

# S2:粗网格,目标函数直接用趟级权重误差(与回放同向,但便宜得多)
best = None
for F in [x / 20 for x in range(0, 41)]:
    for Rp in [y / 100 for y in range(1, 41)]:
        for ratio in [z / 100 for z in range(5, 105, 5)]:
            Rf = Rp * ratio
            e = 0.0
            for job in jobs:
                tot = sum(r["secs"] for r in job)
                w = [F + (Rp if r["kind"] == "pixel" else Rf) * r["pxf"] for r in job]
                tw = sum(w) or 1e-9
                e += sum(fabs(a / tw - r["secs"] / tot) for a, r in zip(w, job))
            if best is None or e < best[0]:
                best = (e, F, Rp, Rf, ratio)
_, F, Rp, Rf, ratio = best
print(f"\nS2 拟合最优:F={F:.2f}s  R_pixel={Rp:.3f}  R_fast={Rf:.3f}(fast 是 pixel 的 {ratio:.0%})")
show("S2 静态成本模型", lambda j: replay(j, "static", F=F, Rp=Rp, Rf=Rf))
show("S3 自适应(等先验)", lambda j: replay(j, "adapt", clamp=False))
show("S4 自适应+钳位", lambda j: replay(j, "adapt", clamp=True))
for p in (1.0, 2.0, 3.0, 5.0, 8.0):
    show(f"S4 自适应+钳位 先验{p}", lambda j, p=p: replay(j, "adapt", clamp=True, prior=p))
