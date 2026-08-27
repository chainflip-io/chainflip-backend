#!/usr/bin/env python3
"""Report on chainflip-node `--tracing-targets` span output, and turn it into a flamegraph.

See "Profiling runtime execution" in the repo README for how to produce the log.

    ./state-chain/scripts/spans-to-profile.py trace.log        # tables + spans.json
    samply load spans.json                 # Firefox Profiler UI, times in ms

The tables cover where the block spends its time, every pallet's `on_initialize` and
`on_finalize`, each elections instance broken down into its electoral systems, and the
extrinsic dispatches - per instance for spans that name one, under `-` for those that do
not. Whatever the log happens to contain is what gets reported - sections with no matching
spans are skipped.

Note that `--tracing-targets` is a strict prefix allowlist: a span whose target matches
none of the listed prefixes never reaches the log at all, so a missing row may mean a
missing target rather than work that did not happen.

The structure of the log is worked out on its own, with nothing to pass on the command
line. `benchmark block` replays each block `--repeat` times (10 by default) and accepts a
range of blocks; blocks are told apart by the `Block N with … tx used …` line the
benchmark logs once a block's repeats are done, which therefore terminates that block's
spans and supplies its extrinsic count and weight. Executions within a block are its root
frames, one per pass.

Every figure is a mean per execution, and the *first* execution of each block is left out
of it: wasmtime compiles the runtime inside that pass, which adds seconds that have
nothing to do with the block. Each block gets its own root frame in the profile, named
after it, so blocks are never averaged together - use `--block N` to report on just one.

Why a converter is needed: samply is a sampling profiler for native code and does
not read these logs, and the runtime's wasm frames carry no symbols on macOS, so a
native recording cannot attribute time to pallets. The spans can - they just need
to be in a format a flamegraph viewer understands.

Spans arrive from wasm with no parent (`parent_id: None`), so the tree is rebuilt
from span ids, which increase on entry, against the log's exit order: span A
encloses B exactly when id_A < id_B and A exits after B.
"""
import argparse
import collections
import json
import re
import sys

LINE = re.compile(r"sc_tracing: (\S+?):? (\w+), time: (\d+), id: (\d+)")
LABEL = re.compile(r'params=" \{ pallet: (\w+) \}"')
# `benchmark block` runs every `--repeat` execution of a block and *then* logs this line
# (bench.rs `run`), so it terminates that block's spans and names the block.
WEIGHT = re.compile(
    r"Block\s+(\d+) with\s+([\d,]+) tx used\s+([\d.]+)% of its weight "
    r"\(\s*([\d,]+) of\s*([\d,]+) ns\)"
)

# (name, colour) — colour names are the ones the profiler front-end understands.
CATEGORIES = [
    ("Other", "grey"),
    ("Runtime", "blue"),
    ("Elections", "orange"),
    ("Witnesser", "green"),
    ("Pallets", "purple"),
]


def category(target):
    if target.startswith("frame_"):
        return 1
    if target == "pallet_cf_elections":
        return 2
    if target == "pallet_cf_witnesser":
        return 3
    if target.startswith("pallet_"):
        return 4
    return 0


def parse(path):
    """Returns (spans, blocks). Every span is tagged with the block it belongs to, taken
    from the weight line that follows it; spans after the last one (a truncated log) are
    tagged `None`."""
    spans, blocks, pending = [], [], []
    for line in open(path):
        m = LINE.search(line)
        if m:
            label = LABEL.search(line)
            pending.append(
                dict(
                    target=m.group(1).rstrip(":").replace("::pallet", ""),
                    name=m.group(2),
                    time=int(m.group(3)),
                    id=int(m.group(4)),
                    label=label.group(1) if label else None,
                    block=None,
                    kids=[],
                )
            )
            continue
        w = WEIGHT.search(line)
        if w:
            num = int(w.group(1))
            for s in pending:
                s["block"] = num
            blocks.append(
                dict(
                    num=num,
                    tx=int(w.group(2).replace(",", "")),
                    percent=float(w.group(3)),
                    took=int(w.group(4).replace(",", "")),
                    consumed=int(w.group(5).replace(",", "")),
                    spans=len(pending),
                )
            )
            spans.extend(pending)
            pending = []
    spans.extend(pending)          # unterminated tail, if the run was cut short
    return spans, blocks


def build_tree(spans):
    """spans are in exit order; ids increase with entry order."""
    stack = []
    for s in spans:
        kids = []
        while stack and stack[-1]["id"] > s["id"]:
            kids.append(stack.pop())
        s["kids"] = list(reversed(kids))
        stack.append(s)
    return stack


def collapse_labelled(n):
    """Merge a labelled span into the identical FRAME span wrapping it, keeping the
    label and the outer (complete) duration."""
    n["kids"] = [collapse_labelled(k) for k in n["kids"]]
    if len(n["kids"]) == 1:
        k = n["kids"][0]
        if k["target"] == n["target"] and k["name"] == n["name"] and n["label"] is None:
            k["time"] = n["time"]
            return k
    return n


def merge_siblings(nodes):
    """Flamegraph merge: identical sibling frames become one, durations summed."""
    groups = collections.OrderedDict()
    for n in nodes:
        groups.setdefault((n["target"], n["name"], n["label"]), []).append(n)
    out = []
    for (target, name, label), group in groups.items():
        out.append(
            dict(
                target=target,
                name=name,
                label=label,
                time=sum(g["time"] for g in group),
                count=len(group),
                kids=merge_siblings([k for g in group for k in g["kids"]]),
            )
        )
    return sorted(out, key=lambda x: -x["time"])


class Profile:
    """Builds the tables of a Firefox Profiler 'processed profile'."""

    def __init__(self):
        self.strings = []
        self._string_idx = {}
        self.funcs = []            # (name_idx, category)
        self._func_idx = {}
        self.frames = []           # func_idx
        self.stacks = []           # (prefix, frame_idx, category)
        self._stack_idx = {}
        self.samples = []          # (stack_idx, time_ms, weight_ms)
        self.clock = 0.0

    def string(self, s):
        if s not in self._string_idx:
            self._string_idx[s] = len(self.strings)
            self.strings.append(s)
        return self._string_idx[s]

    def frame(self, label, category):
        if label not in self._func_idx:
            self._func_idx[label] = len(self.funcs)
            self.funcs.append((self.string(label), category))
            self.frames.append(len(self.funcs) - 1)
        return self._func_idx[label]

    def stack(self, prefix, frame_idx, category):
        key = (prefix, frame_idx)
        if key not in self._stack_idx:
            self._stack_idx[key] = len(self.stacks)
            self.stacks.append((prefix, frame_idx, category))
        return self._stack_idx[key]

    def sample(self, stack_idx, weight_ms):
        if weight_ms <= 0:
            return
        self.samples.append((stack_idx, self.clock, weight_ms))
        self.clock += weight_ms

    def walk(self, node, prefix=None):
        """Depth-first, self-time sample first, so each subtree stays contiguous and
        the Stack Chart reads as a waterfall."""
        label = f"{node['target']}::{node['name']}" if node["target"] else node["name"]
        if node["label"]:
            label += f" [{node['label']}]"
        if node["count"] > 1:
            label += f" x{node['count']}"
        cat = category(node["target"])
        stack_idx = self.stack(prefix, self.frame(label, cat), cat)
        self.sample(stack_idx, (node["time"] - sum(k["time"] for k in node["kids"])) / 1e6)
        for k in node["kids"]:
            self.walk(k, stack_idx)

    def to_json(self, product, thread_name):
        end = self.clock
        return {
            "meta": {
                "version": 24,
                "preprocessedProfileVersion": 49,
                "interval": 1.0,
                "startTime": 0,
                "processType": 0,
                "product": product,
                "categories": [
                    {"name": n, "color": c, "subcategories": ["Other"]} for n, c in CATEGORIES
                ],
                "sampleUnits": {"time": "ms", "eventDelay": "ms", "threadCPUDelta": "µs"},
                "markerSchema": [],
                "pausedRanges": [],
                "symbolicated": True,
                "debug": False,
                "usesOnlyOneStackType": True,
            },
            "libs": [],
            "pages": [],
            "threads": [
                {
                    "name": thread_name,
                    "isMainThread": True,
                    "processName": product,
                    "processType": "default",
                    "processStartupTime": 0,
                    "processShutdownTime": end,
                    "registerTime": 0,
                    "unregisterTime": None,
                    "pausedRanges": [],
                    "showMarkersInTimeline": False,
                    "pid": "0",
                    "tid": "0",
                    "stringArray": self.strings,
                    "samples": {
                        "length": len(self.samples),
                        "weightType": "tracing-ms",
                        "stack": [s[0] for s in self.samples],
                        "time": [s[1] for s in self.samples],
                        "weight": [s[2] for s in self.samples],
                    },
                    "stackTable": {
                        "length": len(self.stacks),
                        "prefix": [s[0] for s in self.stacks],
                        "frame": [s[1] for s in self.stacks],
                        "category": [s[2] for s in self.stacks],
                        "subcategory": [0] * len(self.stacks),
                    },
                    "frameTable": {
                        "length": len(self.frames),
                        "address": [-1] * len(self.frames),
                        "inlineDepth": [0] * len(self.frames),
                        "category": [self.funcs[f][1] for f in self.frames],
                        "subcategory": [0] * len(self.frames),
                        "func": list(self.frames),
                        "nativeSymbol": [None] * len(self.frames),
                        "innerWindowID": [None] * len(self.frames),
                        "implementation": [None] * len(self.frames),
                        "line": [None] * len(self.frames),
                        "column": [None] * len(self.frames),
                    },
                    "funcTable": {
                        "length": len(self.funcs),
                        "name": [f[0] for f in self.funcs],
                        "isJS": [False] * len(self.funcs),
                        "relevantForJS": [False] * len(self.funcs),
                        "resource": [-1] * len(self.funcs),
                        "fileName": [None] * len(self.funcs),
                        "lineNumber": [None] * len(self.funcs),
                        "columnNumber": [None] * len(self.funcs),
                    },
                    "resourceTable": {"length": 0, "lib": [], "name": [], "host": [], "type": []},
                    "nativeSymbols": {
                        "length": 0, "address": [], "functionSize": [], "libIndex": [], "name": [],
                    },
                    "markers": {
                        "length": 0, "category": [], "data": [], "endTime": [], "name": [],
                        "phase": [], "startTime": [],
                    },
                }
            ],
        }


def scale(nodes, divisor):
    """Divide durations and merge counts through the whole tree, turning summed
    totals into per-execution averages."""
    for n in nodes:
        n["time"] /= divisor
        n["count"] = max(round(n["count"] / divisor), 1)
        scale(n["kids"], divisor)


def analyse(spans, blocks):
    """Work out the shape of the log by itself: which spans belong to which block, how
    many times each block was replayed, and the average execution of each.

    A block's executions are its root frames - normally `frame_executive::execute_block`,
    one per `--repeat` pass. The *first* pass is thrown away, because wasmtime compiles
    the runtime inside it and that shows up as several extra seconds that have nothing to
    do with the block. When the filter captured no frame enclosing a whole execution the
    roots are a mixed bag, there is no execution boundary to find, and everything is
    reported as one execution instead."""
    by_block = collections.defaultdict(list)
    for s in spans:
        by_block[s["block"]].append(s)

    out = []
    for b in blocks or [dict(num=None, tx=None, percent=None, took=None, spans=len(spans))]:
        group = by_block.get(b["num"], [])
        if not group:
            continue
        roots = [collapse_labelled(r) for r in build_tree(group)]
        uniform = len({(r["target"], r["name"]) for r in roots}) == 1

        if uniform and len(roots) > 1:
            ordered = sorted(roots, key=lambda r: r["id"])
            kept, dropped, executions = ordered[1:], 1, len(ordered)
        else:
            kept, dropped, executions = roots, 0, len(roots) if uniform else None

        merged = merge_siblings(kept)
        if executions is not None:
            scale(merged, len(kept))
        total = sum(m["time"] for m in merged)
        out.append(
            dict(
                num=b["num"], tx=b["tx"], percent=b["percent"], took=b["took"],
                spans=len(group), executions=executions, dropped=dropped, averaged=len(kept),
                # one root frame per block, named after it, so a profile covering several
                # blocks keeps them side by side instead of averaging them together
                root=dict(target="", name=f"block {b['num']}" if b["num"] else "block",
                          label=None, time=total, count=1, kids=merged),
            )
        )
    return out


def render(headers, rows, aligns):
    """A box-drawn table. Cells are pre-formatted strings."""
    width = [
        max(len(headers[i]), max((len(r[i]) for r in rows), default=0)) for i in range(len(headers))
    ]
    def rule(left, mid, right):
        return left + mid.join("─" * (w + 2) for w in width) + right
    def line(cells):
        return "│ " + " │ ".join(
            c.ljust(w) if a == "l" else c.rjust(w) for c, w, a in zip(cells, width, aligns)
        ) + " │"
    return "\n".join(
        [rule("┌", "┬", "┐"), line(headers), rule("├", "┼", "┤")]
        + [line(r) for r in rows]
        + [rule("└", "┴", "┘")]
    )


def section(title, headers, rows, aligns):
    if not rows:
        return
    print(f"\n{title}")
    print(render(headers, rows, aligns))


def walk(nodes):
    for n in nodes:
        yield n
        yield from walk(n["kids"])


def ms(v):
    return f"{v / 1e6:.2f}"


def short(system, instance):
    """`TronDepositChannelWitnessingES` under `Tron` -> `DepositChannelWitnessing`."""
    name = system[:-2] if system.endswith("ES") else system
    return name[len(instance):] if name.startswith(instance) and len(name) > len(instance) else name


def report_blocks(analysed):
    """The structure the script found in the log, before any of the detail."""
    rows = []
    for a in analysed:
        total = a["root"]["time"] / 1e6
        rows.append([
            str(a["num"]) if a["num"] else "?",
            f"{a['tx']:,}" if a["tx"] is not None else "?",
            f"{a['percent']:.2f}%" if a["percent"] is not None else "?",
            str(a["executions"]) if a["executions"] is not None else "?",
            str(a["averaged"]) if a["executions"] is not None else "?",
            f"{a['spans']:,}",
            f"{total:.2f}",
            f"{a['took'] / 1e6:.2f}" if a["took"] is not None else "?",
        ])
    section("Blocks in this log", ["block", "extrinsics", "weight", "executions", "averaged",
                                   "spans", "ms per execution", "benchmark avg ms"], rows,
            "lrrrrrrr")
    if any(a["dropped"] for a in analysed):
        print("`averaged` excludes the first execution of each block: wasmtime compiles the "
              "runtime\ninside it, which the benchmark's own average still carries.")
    if any(a["executions"] is None for a in analysed):
        print("executions unknown: no frame in the log encloses a whole execution, so everything "
              "is\nreported as one. Add `frame=off` to --tracing-targets to capture "
              "`execute_block`.")


def report(a, spans_in_block):
    root = a["root"]
    total = root["time"]
    head = f"Block {a['num']}" if a["num"] else "Log"
    if a["tx"] is not None:
        head += f" - {a['tx']:,} extrinsics, {a['percent']:.2f}% of its weight"
    print(f"\n{'=' * 78}\n{head}")
    if a["executions"]:
        print(f"{spans_in_block:,} spans, {total / 1e6:.2f} ms per execution "
              f"(mean of {a['averaged']} of {a['executions']} executions)")
    else:
        print(f"{spans_in_block:,} spans, {total / 1e6:.2f} ms total - execution boundaries "
              f"unknown, so this is every replay pooled")
    print("=" * 78)

    roots = root["kids"]

    # ---- where the time goes: the execution root and the frames directly inside it ----
    rows = []
    for r in sorted(roots, key=lambda x: -x["time"]):
        rows.append([f"{r['target']}::{r['name']}", ms(r["time"]),
                     f"{100 * r['time'] / total:.1f}%", str(r["count"])])
        for k in sorted(r["kids"], key=lambda x: -x["time"])[:12]:
            label = f"  {k['target']}::{k['name']}" + (f" [{k['label']}]" if k["label"] else "")
            rows.append([label, ms(k["time"]), f"{100 * k['time'] / total:.1f}%",
                         str(k["count"])])
    section("Block structure (root and the frames directly inside it)",
            ["frame", "total ms", "share", "count"], rows, "lrrr")

    # ---- every pallet's hooks: FRAME's own spans, so `label` is unset ----
    hooks = collections.defaultdict(lambda: {"on_initialize": [0, 0], "on_finalize": [0, 0]})
    for n in walk(roots):
        if n["name"] in ("on_initialize", "on_finalize") and not n["label"]:
            e = hooks[n["target"]][n["name"]]
            e[0] += n["time"]
            e[1] += n["count"]
    rows = []
    for target, h in sorted(hooks.items(), key=lambda kv: -(kv[1]["on_initialize"][0] +
                                                            kv[1]["on_finalize"][0])):
        ini, fin = h["on_initialize"], h["on_finalize"]
        rows.append([target, ms(ini[0]), str(ini[1]) if ini[1] else "-",
                     ms(fin[0]), str(fin[1]) if fin[1] else "-", ms(ini[0] + fin[0])])
    section("Pallet hooks", ["pallet", "on_init ms", "n", "on_final ms", "n", "total ms"],
            rows, "lrrrrr")

    # ---- elections instances, split into electoral systems ----
    rows, detail = [], []
    for n in sorted((n for n in walk(roots) if n["name"] == "on_finalize" and n["label"]),
                    key=lambda x: -x["time"]):
        instance = n["label"].replace("Elections", "")
        systems = sorted(((k["name"], k["time"]) for k in n["kids"]
                          if k["target"].startswith("state_chain_runtime")),
                         key=lambda x: -x[1])
        covered = sum(t for _, t in systems)
        biggest = ", ".join(f"{short(s, instance)} {t / 1e6:.2f}" for s, t in systems[:3]) or "-"
        rows.append([instance, ms(n["time"]), ms(covered) if systems else "-",
                     ms(n["time"] - covered) if systems else "-", biggest])
        for s, t in systems:
            detail.append([instance, short(s, instance), ms(t),
                           f"{100 * t / n['time']:.1f}%"])
    section("Elections instances", ["instance", "on_finalize", "in systems", "unattributed",
                                    "biggest systems (ms)"], rows, "lrrrl")
    section("Electoral systems", ["instance", "system", "ms", "share of instance"], detail, "llrr")

    # ---- extrinsic dispatches ----
    #
    # A span carrying a `pallet` field is attributed to that instance. Spans without one belong
    # here too, under instance `-`: the batched vote path (`submit_election_votes` -> `vote_all`
    # -> `authorise_voter`) is instance-agnostic by design, which is the whole point of batching,
    # so keying the table on the label alone hid exactly the spans worth measuring. Their `call`
    # is qualified with the target, since a bare name is not unique across crates.
    #
    # Two kinds are left out because they already have a table of their own and repeating them
    # here buries everything else: the electoral systems (`Electoral systems` below) and FRAME's
    # own scaffolding (`Block structure`, and they are the tree rather than work in it).
    electoral_systems = {
        id(k)
        for n in walk(roots) if n["name"] == "on_finalize" and n["label"]
        for k in n["kids"] if k["target"].startswith("state_chain_runtime")
    }
    calls = collections.defaultdict(lambda: [0, 0])
    for n in walk(roots):
        if n["name"] in ("on_initialize", "on_finalize"):
            continue
        if id(n) in electoral_systems or category(n["target"]) == 1:
            continue
        instance = n["label"].replace("Elections", "") if n["label"] else "-"
        call = n["name"] if n["label"] else f"{n['target']}::{n['name']}"
        e = calls[(instance, call)]
        e[0] += n["time"]
        e[1] += n["count"]
    rows = [[inst, call, str(c), ms(t), f"{t / c / 1000:.1f}" if c else "-"]
            for (inst, call), (t, c) in sorted(calls.items(), key=lambda kv: -kv[1][0])]
    section("Extrinsic dispatches", ["instance", "call", "count", "total ms", "mean µs"],
            rows, "llrrr")


def main():
    ap = argparse.ArgumentParser(
        description="Turn chainflip-node span logs into a flamegraph.",
        epilog="See 'Profiling runtime execution' in the repo README.",
    )
    ap.add_argument("log", help="output of `benchmark block --tracing-targets=...`")
    ap.add_argument("out", nargs="?", default="spans.json",
                    help="profile to write for `samply load` (default: %(default)s)")
    ap.add_argument("--no-tables", action="store_true",
                    help="skip the stdout report and only write the profile")
    ap.add_argument("--block", type=int, metavar="N",
                    help="only report on block N (default: every block in the log, each on its own)")
    ap.add_argument("--thread-name", default="per execution",
                    help="track name shown in the profiler (default: %(default)s)")
    args = ap.parse_args()

    spans, blocks = parse(args.log)
    if not spans:
        sys.exit(f"no `sc_tracing:` span lines found in {args.log} - was the runtime built with "
                 "`runtime-tracing`, and did the filter enable `wasm_tracing=trace`?")

    untagged = sum(1 for s in spans if s["block"] is None)
    if untagged and blocks:
        print(f"note: ignoring {untagged} span(s) after the last complete block", file=sys.stderr)

    analysed = analyse(spans, blocks)
    if args.block is not None:
        analysed = [a for a in analysed if a["num"] == args.block]
        if not analysed:
            covers = ", ".join(str(b["num"]) for b in blocks) or "no complete block"
            sys.exit(f"block {args.block} is not in {args.log}. It covers: {covers}")

    if not args.no_tables:
        report_blocks(analysed)
        for a in analysed:
            report(a, a["spans"])

    roots = [a["root"] for a in analysed]
    total = sum(r["time"] for r in roots) / 1e6
    prof = Profile()
    for r in roots:
        prof.walk(r)
    json.dump(prof.to_json("chainflip-node", args.thread_name), open(args.out, "w"))
    print(f"\n{len(spans)} spans -> {len(prof.samples)} samples, {len(prof.frames)} frames, "
          f"{total:.2f} ms across {len(roots)} block(s)")
    print(f"wrote {args.out}  —  view with:  samply load {args.out}")


if __name__ == "__main__":
    main()
