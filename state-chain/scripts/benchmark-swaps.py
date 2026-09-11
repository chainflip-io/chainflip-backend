#!/usr/bin/env python3
"""Measure swap_single_leg configurations with FRAME; retain raw samples and a CSV."""

import argparse
import csv
import hashlib
import json
import math
import platform
import statistics
import subprocess
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CONFIGURATIONS = {
    "partial_many_lps": 0,
    "partial_one_lp": 1,
    "full_tick": 2,
    "full_12_per_tick": 3,
    "untouched_ticks": 4,
    "untouched_opposite_side": 5,
    "one_atom_input": 6,
    "price_impact_rollback": 7,
    "partial_existing_accounts": 8,
    "full_one_per_tick": 9,
    "fixed_swap_value": 10,
}
PAIRS = {"btc_sell": 0, "btc_buy": 1, "eth_sell": 2, "eth_buy": 3}


def summarize(batches, components, read_us, write_us):
    """FRAME times are ns; DB weights are additional reference costs, not timings."""
    if len(batches) != 1 or batches[0]["benchmark"] != "swap_single_leg":
        raise ValueError("Expected exactly one swap_single_leg benchmark")
    batch = batches[0]
    times, db = batch["time_results"], batch["db_results"]
    if not times or not db:
        raise ValueError("Missing timing or storage samples")
    if any(dict(sample["components"]) != components for sample in times + db):
        raise ValueError("Benchmark ran different components than requested")
    elapsed = [sample["extrinsic_time"] / 1_000_000 for sample in times]
    reads = max(sample["reads"] for sample in db)
    writes = max(sample["writes"] for sample in db)
    return {
        "samples": len(times),
        "median_ms": statistics.median(elapsed),
        "max_ms": max(elapsed),
        "max_storage_root_ms": max(sample["storage_root_time"] for sample in times) / 1_000_000,
        "reads": reads,
        "repeat_reads": max(sample["repeat_reads"] for sample in db),
        "writes": writes,
        "repeat_writes": max(sample["repeat_writes"] for sample in db),
        "proof_bytes": max(sample["proof_size"] for sample in db),
        "reference_ms": max(elapsed) + (reads * read_us + writes * write_us) / 1_000,
    }


def positive_int(value):
    value = int(value)
    if value < 1:
        raise argparse.ArgumentTypeError("Must be positive")
    return value


def markdown_report(metadata, rows):
    args = metadata["arguments"]
    lines = [
        "### Swap benchmark measurements", "",
        f"Status: **{metadata['status']}**. Completed points: **{len(rows)}**.", "",
        f"Budget screen: {args['budget_ms']:g} ms, {args['attempts']} attempt(s), "
        f"{args['margin']:g}× margin. Reference cost includes configured DB weights; "
        "it is not a wall-clock measurement or a proven runtime limit.", "",
        "| Pair | Configuration | USD/order | Orders | Median ms | Max ms | Reads | Writes | Reference ms | Below budget |",
        "|---|---|---:|---:|---:|---:|---:|---:|---:|---|",
    ]
    for row in rows:
        lines.append(
            f"| {row['pair']} | {row['configuration']} | {row['order_usd']} | {row['n']} | "
            f"{row['median_ms']:.3f} | {row['max_ms']:.3f} | {row['reads']} | {row['writes']} | "
            f"{row['reference_ms']:.3f} | {'yes' if row['below_budget'] else 'no'} |")
    lines += ["", "Orders excludes the extra guard order. See the artifact for raw samples, "
              "storage-root times, proof sizes, logs and the run manifest."]
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/chainflip-node")
    parser.add_argument("--binary-commit", help="Source commit of a downloaded binary (may differ from checkout)")
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--orders", type=positive_int, nargs="+", default=[1, 12, 100, 1_000, 10_000])
    parser.add_argument("--order-usd", type=positive_int, nargs="+", default=[10])
    parser.add_argument("--pairs", choices=PAIRS, nargs="+", default=list(PAIRS))
    parser.add_argument("--configurations", choices=CONFIGURATIONS, nargs="+", default=list(CONFIGURATIONS))
    parser.add_argument("--repeat", type=positive_int, default=10)
    parser.add_argument("--budget-ms", type=float, default=1_000)
    parser.add_argument("--attempts", type=positive_int, default=1, help="Swap legs/attempts sharing the budget")
    parser.add_argument("--margin", type=float, default=2, help="Multiplier on the reference cost")
    parser.add_argument("--db-read-us", type=float, default=8, help="ParityDbWeight read cost")
    parser.add_argument("--db-write-us", type=float, default=50, help="ParityDbWeight write cost")
    parser.add_argument("--timeout", type=positive_int, default=600, help="Seconds per node invocation")
    parser.add_argument("--heap-pages", type=positive_int, help="Override WASM heap pages (64 KiB each)")
    args = parser.parse_args()
    if max(args.orders) > 100_000 or max(args.order_usd) > 1_000_000:
        parser.error("Maximum supported orders: 100000; order USD: 1000000")
    if (not all(map(math.isfinite, [args.budget_ms, args.margin, args.db_read_us, args.db_write_us]))
            or args.budget_ms <= 0 or args.margin < 1 or min(args.db_read_us, args.db_write_us) < 0):
        parser.error("Require positive budget, margin >= 1 and nonnegative DB costs")
    binary = args.binary.resolve(strict=True)
    output_dir = args.output_dir.resolve()
    # Refuse to mix measurements from different binaries/runs.
    output_dir.mkdir(parents=True, exist_ok=False)
    binary_hash = hashlib.sha256()
    with binary.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            binary_hash.update(chunk)
    metadata = {
        "status": "running",
        "utc": datetime.now(timezone.utc).isoformat(),
        "host": platform.uname()._asdict(),
        "binary": str(binary),
        "binary_sha256": binary_hash.hexdigest(),
        "binary_commit": args.binary_commit,
        "git_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "git_status": subprocess.check_output(["git", "status", "--short"], cwd=ROOT, text=True),
        "arguments": {key: str(value) if isinstance(value, Path) else value for key, value in vars(args).items()},
        "runs": [],
    }
    rows = []
    limits = []

    def save():
        (output_dir / "manifest.json").write_text(json.dumps(metadata, indent=2) + "\n")
        if rows:
            with (output_dir / "results.csv").open("w", newline="") as stream:
                writer = csv.DictWriter(stream, fieldnames=list(rows[0]))
                writer.writeheader()
                writer.writerows(rows)
        (output_dir / "budget-screen.json").write_text(json.dumps(limits, indent=2) + "\n")
        (output_dir / "summary.md").write_text(markdown_report(metadata, rows))

    for pair in args.pairs:
        for configuration in args.configurations:
            for dollars in args.order_usd:
                screen = {"pair": pair, "configuration": configuration, "order_usd": dollars,
                          "largest_tested_n_below_budget": None, "first_tested_n_over_budget": None}
                limits.append(screen)
                for n in sorted(set(args.orders)):
                    stem = f"{pair}-{configuration}-{dollars}usd-{n}orders"
                    components = {"n": n, "c": CONFIGURATIONS[configuration], "a": PAIRS[pair], "v": dollars}
                    pinned = ",".join(map(str, components.values()))
                    raw = output_dir / f"{stem}.json"
                    log = output_dir / f"{stem}.log"
                    command = [str(binary), "benchmark", "pallet", "--chain=dev-3",
                               "--pallet=pallet_cf_pools", "--extrinsic=swap_single_leg", "--extra",
                               f"--low={pinned}", f"--high={pinned}", "--steps=2",
                               "--repeat=1", f"--external-repeat={args.repeat}",
                               "--wasm-execution=compiled", "--no-median-slopes", "--no-min-squares",
                               f"--json-file={raw}"]
                    if args.heap_pages is not None:
                        command.append(f"--heap-pages={args.heap_pages}")
                    run = {"command": command, "status": "running"}
                    metadata["runs"].append(run)
                    save()
                    print(stem, flush=True)
                    try:
                        with log.open("w") as stream:
                            subprocess.run(command, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT,
                                           timeout=args.timeout, check=True)
                        metrics = summarize(json.loads(raw.read_text()), components, args.db_read_us, args.db_write_us)
                    except (subprocess.SubprocessError, ValueError, OSError, KeyError) as error:
                        metadata["status"] = "failed"
                        run["status"] = "failed"
                        run["error"] = str(error)
                        save()
                        raise SystemExit(f"Benchmark failed; inspect {log}: {error}") from error
                    run["status"] = "complete"
                    cost = metrics["reference_ms"] * args.attempts * args.margin
                    rows.append({"pair": pair, "configuration": configuration, "order_usd": dollars,
                                 "n": n, "stored_orders": n + 1, **metrics, "budget_cost_ms": cost,
                                 "below_budget": cost <= args.budget_ms})
                    print(f"  max {metrics['max_ms']:.3f} ms; reference {metrics['reference_ms']:.3f} ms; "
                          f"with attempts/margin {cost:.3f}/{args.budget_ms:g} ms", flush=True)
                    if cost > args.budget_ms:
                        screen["first_tested_n_over_budget"] = n
                        save()
                        break
                    screen["largest_tested_n_below_budget"] = n
                    save()
    metadata["status"] = "complete"
    save()
    print(f"Results: {output_dir / 'results.csv'}")
    print("Budget screen uses tested points only; it is not an enforced or proven runtime limit.")


if __name__ == "__main__":
    main()
