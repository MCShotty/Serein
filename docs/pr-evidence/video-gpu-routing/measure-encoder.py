#!/usr/bin/env python3
"""Compare actual baseline/new software camera GOPs. No renderer/GPU/capture/network."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path, required=True)
    parser.add_argument("--baseline", default="79e336066beff5ea6c414268308f1f817c50c2f4")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[3]
    prefix = args.prefix.resolve()
    source = repo / "crates/discord-voice/src/video_encode_ffmpeg.c"
    benchmark = Path(__file__).with_name("encoder-bench.c")
    args.output.mkdir(parents=True, exist_ok=True)
    rows = []
    with tempfile.TemporaryDirectory(prefix="serein-gop-bench-", dir=args.output) as directory:
        work = Path(directory)
        old = work / "before.c"
        old.write_bytes(subprocess.check_output(["git", "show", args.baseline + ":crates/discord-voice/src/video_encode_ffmpeg.c"], cwd=repo))
        commands = []
        for name, shim in [("before", old), ("after", source)]:
            command = ["cc", "-std=c11", "-O2", "-Wall", "-Wextra", "-Werror",
                       '-DSHIM="' + str(shim) + '"', "-I" + str(source.parent), "-I" + str(prefix / "include"),
                       str(benchmark), "-L" + str(prefix / "lib"), "-Wl,-rpath," + str(prefix / "lib"),
                       "-lavcodec-serein", "-lavutil-serein", "-o", str(work / name)]
            subprocess.run(command, check=True)
            commands.append(command)
        for run in range(6):
            for name, baseline in [("before", 1), ("after", 0)]:
                executable = work / name
                bitstream = args.output / (name + ".h264")
                process = subprocess.Popen([str(executable), str(baseline), str(bitstream)], stdout=subprocess.PIPE, text=True)
                peak = 0
                samples = 0
                while process.poll() is None:
                    try:
                        status = Path(f"/proc/{process.pid}/status").read_text()
                        rss = next(line for line in status.splitlines() if line.startswith("VmRSS:"))
                        peak = max(peak, int(rss.split()[1]) * 1024)
                        samples += 1
                    except (FileNotFoundError, StopIteration, ProcessLookupError):
                        pass
                    time.sleep(0.01)
                stdout, _ = process.communicate(timeout=5)
                if process.returncode:
                    raise RuntimeError(f"{name} failed: {process.returncode}")
                row = dict(json.loads(stdout), name=name, run=run, sampled_peak_rss_bytes=peak, rss_samples=samples)
                print(row, flush=True)
                rows.append(row)
        metadata = {
            "baseline": args.baseline, "current_base": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
            "source_sha256": digest(source), "baseline_source_sha256": digest(old), "benchmark_sha256": digest(benchmark),
            "ffmpeg_prefix": str(prefix), "compiler": subprocess.check_output(["cc", "--version"], text=True).splitlines()[0],
            "environment": {"kernel": os.uname().release, "cpus": os.cpu_count()},
            "sampling_interval_ms": 10, "warmup_pictures": 30, "measured_pictures": 300,
            "warmup_runs_discarded": 1, "measured_runs": 5, "commands": commands, "runs": rows,
            "medians": {name: {metric: statistics.median(row[metric] for row in rows if row["name"] == name and row["run"] > 0)
                                for metric in ["elapsed_ms", "encoded_bytes", "keyframes", "sampled_peak_rss_bytes"]}
                        for name in ["before", "after"]},
            "bitstream_sha256": {name: digest(args.output / (name + ".h264")) for name in ["before", "after"]},
        }
        (args.output / "measurements.json").write_text(json.dumps(metadata, indent=2) + "\n")


if __name__ == "__main__":
    main()
