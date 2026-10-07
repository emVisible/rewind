#!/usr/bin/env python3
"""#21 参数上限实测化:对高影响参数跑边界,量"到哪儿开始坏 / 到哪儿贵得离谱"。

用法:
  python3 scripts/safe_max.py dump                 # 看清单里现有的 min/max
  python3 scripts/safe_max.py sweep [预设]         # 跑边界扫描,输出 .work/gate/safe_max.tsv
判据三条(缺一不算"实测过"):
  1) 引擎要么 rc=0 要么给出**说人话的拒绝**(不能崩、不能静默出怪图);
  2) 成品必须能解码、时长与帧数对得上;
  3) 记录 wall 与字节:上限不能只看"没报错",还要看"没贵到不能用"。
"""
import json, os, subprocess, sys, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.join(ROOT, "core", "target", "release", "rewind-core")
SRC = os.path.join(ROOT, "assets", "sample", "street-food.mp4")
OUT = os.path.join(ROOT, ".work", "gate", "safe_max")

# 扫描表:参数 → 取值序列(含明显越界的值,验证"拒绝是否说人话")
SWEEP = {
    "noise.alls":        [0, 12, 40, 100, 2000, -5],
    "unsharp.amount":    [0, 0.6, 1.5, 3.0, 20.0, -1],
    "band_quantize.level": [24, 16, 8, 2, 1, 0, 255],
    "resize.overscan":   [1.0, 0.94, 0.8, 0.59, 0.2],
    "fps.fps":           [25, 15, 6, 1, 0.5, 0, 1000],
    "fps.shutter":       [0.5, 1.0, 2.0, 10.0, -1],
    "codec_roundtrip.q": [10, 2, 1, 0, 31, 65535],
    "color.saturation":  [1.0, 0.0, 2.5, 50.0, -3],
    "tape_ends.head":    [0.6, 3.0, 8.0, 600.0, -1],
}


def sh(args, timeout=180):
    t0 = time.time()
    try:
        p = subprocess.run(args, capture_output=True, text=True, timeout=timeout)
        return p.returncode, p.stdout, p.stderr, time.time() - t0
    except subprocess.TimeoutExpired:
        return 124, "", "TIMEOUT", time.time() - t0


def probe_dims(path):
    rc, out, _, _ = sh(["ffprobe", "-v", "error", "-select_streams", "v:0",
                        "-show_entries", "stream=width,height,nb_frames", "-of", "csv=p=0", path])
    return out.strip().replace("\n", " ")


def dump():
    rc, out, _, _ = sh([BIN, "describe"])
    d = json.loads(out)
    print("%-18s %-12s %-8s %-8s %-8s %s" % ("stage", "key", "min", "max", "default", "kind"))
    for sec in ("video", "audio"):
        for st in d.get(sec) or []:
            for p in st.get("params") or []:
                key = "%s.%s" % (st["stage"], p["key"])
                if key in SWEEP:
                    print("%-18s %-12s %-8s %-8s %-8s %s" % (
                        st["stage"], p["key"], p.get("min"), p.get("max"), p.get("default"), p.get("kind")))


def sweep(preset="presets/patina.json"):
    os.makedirs(OUT, exist_ok=True)
    rows = []
    for param, values in SWEEP.items():
        stage, key = param.split(".", 1)
        for v in values:
            args = [BIN, "run", "--preset", os.path.join(ROOT, preset), "--input", SRC,
                    "--out-dir", OUT, "--override", "%s.%s=%s" % (stage, key, v)]
            rc, out, err, wall = sh(args)
            ok_line = [l for l in out.splitlines() if '"output"' in l]
            produced = json.loads(ok_line[-1])["output"] if ok_line else ""
            decoded = ""
            if produced and os.path.exists(produced):
                drc, _, derr, _ = sh(["ffmpeg", "-v", "error", "-xerror", "-i", produced, "-f", "null", "-"])
                decoded = "ok" if drc == 0 else "BAD:" + derr.strip()[:30]
            msg = ""
            if rc != 0:
                msg = (json.loads([l for l in out.splitlines() if '"error"' in l][0])["error"]
                       if any('"error"' in l for l in out.splitlines())
                       else (err.strip() or out.strip())[:90])
            size = os.path.getsize(produced) if produced and os.path.exists(produced) else 0
            rows.append((preset.split("/")[-1][:-5], param, v, rc, round(wall, 2), size, decoded, msg))
            print("%-14s %-20s %-8s rc=%-4s %6.2fs %9d B %-10s" % rows[-1][:7] + ("  " + msg if msg else ""))
    with open(os.path.join(OUT, "safe_max.tsv"), "w") as f:
        f.write("preset\tparam\tvalue\trc\twall_s\tbytes\tdecoded\terror\n")
        for r in rows:
            f.write("\t".join(str(x) for x in r) + "\n")
    print("\n报告写入 %s" % os.path.join(OUT, "safe_max.tsv"))


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "dump"
    if cmd == "dump":
        dump()
    else:
        sweep(sys.argv[2] if len(sys.argv) > 2 else "presets/patina.json")
