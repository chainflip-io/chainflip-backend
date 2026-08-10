#!/usr/bin/env python3
"""Extract block authorship timings from `chainflip-node` logs into a CSV for plotting.

The node logs one line per block it proposes (sc_basic_authorship, INFO):

    2026-07-30 12:12:18.078  INFO tokio-rt-worker sc_basic_authorship::basic_authorship:
    🎁 Prepared block for proposing at 1351 (72 ms) hash: 0xd350…; parent_hash: 0x186e…7034;
    end: NoMoreTransactions; extrinsics_count: 6

This turns those into a tidy table - one row per proposed block - so block construction
time can be plotted against the number of extrinsics in the block.

    ./state-chain/scripts/block-authorship.py process node.log -o blocks.csv
    ./state-chain/scripts/block-authorship.py process '/tmp/chainflip/*/*node*.log' -o out.csv
    ./state-chain/scripts/block-authorship.py process node.log | ...

`process` takes any number of files, shell globs, or `-` for stdin, and reads `.gz`
transparently. It writes the CSV to `-o`, or to stdout if `-o` is omitted; the summary
goes to stderr either way, so the CSV can be piped.

To reprint that summary later without touching the logs again, hand the CSV back to
`summarize` (which goes to stdout, since nothing competes for it):

    ./state-chain/scripts/block-authorship.py summarize blocks.csv

Columns:

    block            block number
    took_ms          how long the node spent building the block (`block_took`)
    extrinsics       number of extrinsics included
    end_reason       why proposing stopped - NoMoreTransactions, HitDeadline,
                     HitBlockWeightLimit, HitBlockSizeLimit, TransactionForbidden.
                     Anything other than NoMoreTransactions means the block was cut
                     short, so `extrinsics` is a floor, not the demand.
    timestamp        log timestamp, ISO-8601
    epoch_ms         same instant in milliseconds, for time-axis plots
    interval_ms      gap to the previously proposed block, from the timestamps
    hash             block hash
    parent_hash      as logged (usually abbreviated)
    source           file the line came from

The interesting relationship is `took_ms ~ a + b * extrinsics`: `b` is the marginal
cost of one extrinsic to the block author, which is exactly what batching reduces.
The summary prints that fit so it can be compared before and after a change.
"""
import argparse
import csv
import glob
import gzip
import io
import re
import statistics
import sys
from datetime import datetime, timezone

# `block_took` and the extrinsic count are the two numbers of interest; the rest of the
# line is matched leniently so a format tweak upstream degrades to missing columns
# rather than dropping the row.
PREPARED = re.compile(r"Prepared block for proposing at (\d+) \((\d+) ms\)")
TIMESTAMP = re.compile(r"^(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d+)")
HASH = re.compile(r"hash: (0x[0-9a-fA-F]+)")
PARENT = re.compile(r"parent_hash: (\S+?);")
END = re.compile(r"end: (\w+)")
# INFO logs `extrinsics_count: N`; TRACE logs `extrinsics (N): [...]` or `no extrinsics`.
COUNT = re.compile(r"extrinsics_count: (\d+)")
COUNT_TRACE = re.compile(r"extrinsics \((\d+)\):")

FIELDS = [
    "block", "took_ms", "extrinsics", "end_reason", "timestamp",
    "epoch_ms", "interval_ms", "hash", "parent_hash", "source",
]

# Shown by `--help` and again at the end of every summary, with `{csv}` filled in with
# the file actually written, so the commands can be pasted as-is. One definition, so the
# help and the summary cannot drift apart.
PLOTTING = """\
Plotting (any tool that reads CSV will do; 'skip 1' drops the CSV header). The canvas is
set wide because a bouncer run is thousands of blocks - swap 'qt' for 'wxt' or 'x11' if
your gnuplot lacks it, or write a file instead with 'set terminal svg size 4000,500;
set output "blocks.svg"' and open that in a browser, which zooms:

    # block construction time, per block number
    gnuplot -p -e "set terminal qt size 1800,500; set datafile separator ','; \\
        plot '{csv}' skip 1 using 1:2 with lines title 'took_ms'"

    # extrinsic count, per block number
    gnuplot -p -e "set terminal qt size 1800,500; set datafile separator ','; \\
        plot '{csv}' skip 1 using 1:3 with lines title 'extrinsics'"

    # both at once, in pandas
    df = pd.read_csv('{csv}')
    df.plot(x='block', y=['took_ms', 'extrinsics'], subplots=True, figsize=(24, 6))\
"""


def open_log(path):
    if path == "-":
        return sys.stdin
    if path.endswith(".gz"):
        return io.TextIOWrapper(gzip.open(path, "rb"), errors="replace")
    return open(path, errors="replace")


def parse_timestamp(line):
    """The node logs local time with no offset, so treat it as naive and only ever use
    it for differences - never for absolute correlation against another machine."""
    m = TIMESTAMP.match(line)
    if not m:
        return None, None
    try:
        dt = datetime.strptime(m.group(1), "%Y-%m-%d %H:%M:%S.%f")
    except ValueError:
        return None, None
    return m.group(1), int(dt.replace(tzinfo=timezone.utc).timestamp() * 1000)


def read_csv_rows(path):
    """Read back a CSV this script wrote, so the summary can be reprinted without
    re-parsing the logs."""
    with open_log(path) as f:
        rows = list(csv.DictReader(f))
    if not rows:
        raise SystemExit(f"{path}: no rows")
    missing = {"block", "took_ms", "extrinsics"} - set(rows[0])
    if missing:
        raise SystemExit(f"{path}: not a block-authorship CSV, missing {sorted(missing)}")
    for r in rows:
        for field in ("block", "took_ms", "extrinsics", "epoch_ms", "interval_ms"):
            value = r.get(field, "")
            r[field] = int(value) if value not in ("", None) else ""
    return rows


def parse(paths):
    rows, skipped = [], 0
    for path in paths:
        with open_log(path) as f:
            for line in f:
                m = PREPARED.search(line)
                if not m:
                    continue
                count = COUNT.search(line) or COUNT_TRACE.search(line)
                if count is None and "no extrinsics" in line:
                    n = 0
                elif count is None:
                    skipped += 1
                    continue
                else:
                    n = int(count.group(1))
                ts, epoch_ms = parse_timestamp(line)
                end = END.search(line)
                h = HASH.search(line)
                p = PARENT.search(line)
                rows.append({
                    "block": int(m.group(1)),
                    "took_ms": int(m.group(2)),
                    "extrinsics": n,
                    "end_reason": end.group(1) if end else "",
                    "timestamp": ts or "",
                    "epoch_ms": epoch_ms if epoch_ms is not None else "",
                    "interval_ms": "",
                    "hash": h.group(1) if h else "",
                    "parent_hash": p.group(1) if p else "",
                    "source": path,
                })
    return rows, skipped


def add_intervals(rows):
    """Gap between consecutive proposals, per source file - interleaving two nodes' logs
    would otherwise produce meaningless intervals."""
    by_source = {}
    for r in sorted(rows, key=lambda r: (r["source"], r["block"])):
        prev = by_source.get(r["source"])
        if prev is not None and r["epoch_ms"] != "" and prev["epoch_ms"] != "":
            r["interval_ms"] = r["epoch_ms"] - prev["epoch_ms"]
        by_source[r["source"]] = r


def fit(xs, ys):
    """Least squares y = a + b*x, plus Pearson r. Returns None if x has no spread."""
    n = len(xs)
    if n < 2:
        return None
    mx, my = statistics.fmean(xs), statistics.fmean(ys)
    sxx = sum((x - mx) ** 2 for x in xs)
    syy = sum((y - my) ** 2 for y in ys)
    sxy = sum((x - mx) * (y - my) for x, y in zip(xs, ys))
    if sxx == 0 or syy == 0:
        return None
    b = sxy / sxx
    return my - b * mx, b, sxy / (sxx * syy) ** 0.5


def summarise(rows, out, csv_path):
    def pct(values, p):
        s = sorted(values)
        return s[min(len(s) - 1, int(round(p / 100 * (len(s) - 1))))]

    took = [r["took_ms"] for r in rows]
    xts = [r["extrinsics"] for r in rows]
    blocks = [r["block"] for r in rows]
    print(f"{len(rows)} proposed blocks, #{min(blocks)}..#{max(blocks)}", file=out)

    print("\n%-12s %8s %8s %8s %8s %8s" % ("", "mean", "median", "p95", "max", "min"), file=out)
    for name, vals in (("took_ms", took), ("extrinsics", xts)):
        print("%-12s %8.1f %8.1f %8.1f %8d %8d" % (
            name, statistics.fmean(vals), statistics.median(vals),
            pct(vals, 95), max(vals), min(vals)), file=out)

    intervals = [r["interval_ms"] for r in rows if r["interval_ms"] != ""]
    if intervals:
        print("%-12s %8.1f %8.1f %8.1f %8d %8d" % (
            "interval_ms", statistics.fmean(intervals), statistics.median(intervals),
            pct(intervals, 95), max(intervals), min(intervals)), file=out)

    reasons = {}
    for r in rows:
        reasons[r["end_reason"] or "?"] = reasons.get(r["end_reason"] or "?", 0) + 1
    print("\nend reasons:", file=out)
    for reason, n in sorted(reasons.items(), key=lambda kv: -kv[1]):
        note = "" if reason == "NoMoreTransactions" else "   <- block cut short"
        print("  %-22s %6d  (%4.1f%%)%s" % (reason, n, 100 * n / len(rows), note), file=out)

    f = fit(xts, took)
    if f:
        a, b, r = f
        print(f"\ntook_ms ~ {a:.1f} + {b:.4f} * extrinsics   (r = {r:.3f}, "
              f"{b * 1000:.0f} us per extrinsic)", file=out)
    else:
        print("\nno fit: extrinsic count does not vary", file=out)

    dupes = len(blocks) - len(set(blocks))
    if dupes:
        print(f"\nnote: {dupes} duplicate block numbers (forks, restarts, or merged logs)",
              file=out)

    print("\n" + PLOTTING.format(csv=csv_path), file=out)


def cmd_summarize(args):
    summarise(read_csv_rows(args.csv), sys.stdout, args.csv)
    return 0


def cmd_process(args):
    paths = []
    for pattern in args.logs:
        matches = [pattern] if pattern == "-" else sorted(glob.glob(pattern))
        if not matches:
            print(f"no files match {pattern!r}", file=sys.stderr)
            return 1
        paths += matches

    rows, skipped = parse(paths)
    if not rows:
        print("no 'Prepared block for proposing' lines found - is this a node log, and "
              "was it logging at INFO?", file=sys.stderr)
        return 1
    if skipped:
        print(f"warning: skipped {skipped} lines with no parseable extrinsic count",
              file=sys.stderr)

    add_intervals(rows)
    rows.sort(key=lambda r: (r["block"], r["source"]))

    out = sys.stdout if args.out == "-" else open(args.out, "w", newline="")
    writer = csv.DictWriter(out, fieldnames=FIELDS)
    writer.writeheader()
    writer.writerows(rows)
    if out is not sys.stdout:
        out.close()
        print(f"wrote {len(rows)} rows to {args.out}\n", file=sys.stderr)

    # When the CSV went to stdout it has no path to paste into a plotting command, so
    # the examples fall back to a placeholder name.
    summarise(rows, sys.stderr, "blocks.csv" if out is sys.stdout else args.out)
    return 0


def main():
    ap = argparse.ArgumentParser(
        description=__doc__ + "\n" + PLOTTING.format(csv="blocks.csv"),
        formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="command", required=True, metavar="COMMAND")

    process = sub.add_parser(
        "process", help="parse node logs into a CSV",
        description="Parse `Prepared block for proposing` lines out of node logs and "
                    "write one CSV row per proposed block. The summary goes to stderr, "
                    "so the CSV can be piped.")
    process.add_argument("logs", nargs="+", metavar="LOG",
                         help="node log files, shell globs, or - for stdin; .gz is read "
                              "transparently")
    process.add_argument("-o", "--out", default="-",
                         help="CSV output path (default: stdout)")
    process.set_defaults(run=cmd_process)

    summarize = sub.add_parser(
        "summarize", help="reprint the summary for a CSV written by `process`",
        description="Reprint the summary for a CSV written by `process`, without "
                    "re-reading the logs. Goes to stdout.")
    summarize.add_argument("csv", metavar="CSV", help="a CSV written by `process`")
    summarize.set_defaults(run=cmd_summarize)

    args = ap.parse_args()
    return args.run(args)


if __name__ == "__main__":
    sys.exit(main())