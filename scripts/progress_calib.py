#!/usr/bin/env python3
"""给总体进度定标:cost = 每趟固定开销 + 每像素-帧边际成本 × (画布像素 × 帧数)。

采集口径:engine 自己的 `plan` 导出里的 basis(画布 + 生效帧率),配实测每趟墙钟。
两种画幅 × 两种时长,把"固定"和"边际"分开;只跑 6 秒短片会把固定开销误算成边际成本。
"""
import json
import subprocess
import sys
import time

BIN = "./core/target/release/rewind-core"
CLIPS = ["fixtures/test_src.mp4", ".work/samples/src320_24s.mp4",
         ".work/samples/src720.mp4", ".work/samples/src720_24s.mp4"]
PRESETS = ["patina", "crt1995", "vhs1990_ntscrs", "film1970", "dvd2005", "phone2010", "cctv2000", "rmvb2006"]
ROWS = ".work/step_cost.jsonl"


def plan_of(preset, clip):
    r = subprocess.run([BIN, "plan", "--preset", f"presets/{preset}.json", "--input", clip],
                       capture_output=True, text=True)
    line = [l for l in r.stdout.splitlines() if l.startswith("{")]
    return json.loads(line[-1]) if line else None


def measure(preset, clip):
    cmd = [BIN, "run", "--preset", f"presets/{preset}.json", "--input", clip, "--out-dir", ".work/samples/timing"]
    t0 = time.monotonic()
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1)
    marks, done = [], None
    for line in p.stdout:
        line = line.strip()
        if not line.startswith("{"):
            continue
        ev = json.loads(line)
        if ev["type"] == "progress":
            marks.append((ev["stage"], time.monotonic() - t0))
        elif ev["type"] == "done":
            done = time.monotonic() - t0
            break
        elif ev["type"] == "error":
            return None, ev
    p.wait()
    if done is None:
        return None, {"error": "no done"}
    order = []
    for st, _ in marks:
        if st not in order:
            order.append(st)
    ends = []
    for i, _ in enumerate(order):
        nxt = next((t for st, t in marks if st == order[i + 1]), None) if i + 1 < len(order) else None
        ends.append(nxt if nxt is not None else done)
    segs, prev = [], 0.0
    for e in ends:
        segs.append(e - prev)
        prev = e
    return segs, None


def main():
    out = open(ROWS, "w")
    for clip in CLIPS:
        dur = float(subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration",
                                    "-of", "csv=p=0", clip], capture_output=True, text=True).stdout.strip())
        for name in PRESETS:
            d = plan_of(name, clip)
            if d is None:
                print(f"skip {name} {clip}: plan 失败")
                continue
            segs, err = measure(name, clip)
            if segs is None:
                print(f"skip {name} {clip}: {err}")
                continue
            steps = d["steps"]
            if len(segs) != len(steps):
                print(f"skip {name} {clip}: 趟数对不上 {len(segs)} vs {len(steps)}")
                continue
            for i, s in enumerate(steps):
                b = s["basis"]
                row = {
                    "preset": name, "clip": clip, "dur": dur, "step": i + 1, "n": len(steps),
                    "kind": s["kind"], "w": b["w"], "h": b["h"], "fps": b["fps"],
                    "px": b["w"] * b["h"], "frames": max(1, round(b["fps"] * dur)),
                    "secs": round(segs[i], 4),
                }
                row["pxf"] = row["px"] * row["frames"] / 1e6
                out.write(json.dumps(row) + "\n")
            out.flush()
            print(f"ok {name:<16} {clip.split('/')[-1]:<18} {len(steps)} 趟  {sum(segs):.1f}s")
    out.close()


if __name__ == "__main__":
    main()
