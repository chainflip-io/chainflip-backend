#!/usr/bin/env python3
"""
Analysis of benchmark weight changes, showing function-level changes.

Usage:
    git diff -U50 main...HEAD state-chain/pallets/*/src/weights.rs | ./state-chain/scripts/analyze-benchmark-weights.py

This script provides function-level analysis of benchmark weight changes,
including which pallets and functions have the most significant changes.

Changes are compared on *worst-case total effective weight*: the base ref_time
from `Weight::from_parts` plus db reads/writes priced at the runtime's DbWeight
(ParityDbWeight), plus every parameterized term evaluated at the top of its
component range. This catches changes where execution time is stable but storage
access counts moved, and changes that live entirely in the per-component terms
(a benchmark that starts scaling with `n` shows up as the increase it is,
instead of as a decrease in the base).
"""

import re
import sys

# Output tuning. A per-function change below NOISE_PCT is treated as benchmark
# measurement noise and folded into a collapsed section rather than the headline.
TOP_N = 30
BIG_CHANGE_PCT = 50.0
NOISE_PCT = 2.0

# ParityDbWeight (the runtime's DbWeight), in picoseconds per operation.
DB_READ_PS = 8_000_000
DB_WRITE_PS = 50_000_000


MAX_UNKNOWN = '?'


def parse_functions(lines):
    """Parse weight functions from one side (old or new) of a diff hunk.

    Returns {function_name: fn}, where `fn` holds the base terms ('ref_time',
    'reads', 'writes') and, keyed by component name, the parameterized terms
    ('coef', 'creads', 'cwrites') along with each component's upper bound
    ('ranges', read from the `The range of component ...` doc comment).
    """
    blocks = []
    current = None
    ranges = {}
    for line in lines:
        # Component ranges are documented immediately above the function they
        # belong to, so whatever has accumulated attaches to the next `fn`.
        range_match = re.search(
            r'The range of component `(\w+)` is `\[\d+, (\d+)\]`', line)
        if range_match:
            ranges[range_match.group(1)] = int(range_match.group(2))
            continue
        fn_match = re.search(r'\bfn\s+(\w+)\s*\(', line)
        if fn_match:
            current = {'ref_time': None, 'reads': 0, 'writes': 0,
                       'ranges': ranges, 'coef': {}, 'creads': {}, 'cwrites': {}}
            blocks.append((fn_match.group(1), current))
            ranges = {}
            continue
        if current is None:
            continue
        # Parameterized terms first: they use the same call names as the base
        # terms and would otherwise be misread as them.
        coef_match = re.search(
            r'Weight::from_parts\((\d[\d_]*),\s*0\)\.saturating_mul\((\w+)\.into\(\)\)', line)
        if coef_match:
            accrue(current['coef'], coef_match.group(2), coef_match.group(1))
            continue
        matched = False
        for field, pattern in (
                ('creads', r'\.reads\(\((\d[\d_]*)_u64\)\.saturating_mul\((\w+)\.into\(\)\)\)'),
                ('cwrites', r'\.writes\(\((\d[\d_]*)_u64\)\.saturating_mul\((\w+)\.into\(\)\)\)')):
            m = re.search(pattern, line)
            if m:
                accrue(current[field], m.group(2), m.group(1))
                matched = True
        if matched:
            continue
        # The base ref_time is the first plain `Weight::from_parts`; the
        # proof-size-only component term (`from_parts(0, N).saturating_mul(..)`)
        # is not it.
        parts_match = re.search(r'Weight::from_parts\((\d[\d_]*)', line)
        if parts_match and current['ref_time'] is None and 'saturating_mul' not in line:
            current['ref_time'] = int(parts_match.group(1).replace('_', ''))
        for field, pattern in (('reads', r'\.reads\((\d[\d_]*)_u64\)'),
                               ('writes', r'\.writes\((\d[\d_]*)_u64\)')):
            m = re.search(pattern, line)
            if m:
                current[field] += int(m.group(1).replace('_', ''))
    # Drop trait declarations and truncated bodies with no weight data. Both
    # `WeightInfo` impls define each function with identical values, so keep
    # the first parsed block per name.
    functions = {}
    for name, f in blocks:
        if f['ref_time'] is not None and name not in functions:
            functions[name] = f
    return functions


def accrue(terms, component, value):
    terms[component] = terms.get(component, 0) + int(value.replace('_', ''))


def components(f):
    return set(f['ranges']) | set(f['coef']) | set(f['creads']) | set(f['cwrites'])


def total_ps(f):
    """Worst case: base terms plus each component term at the top of its range.

    A component whose range didn't make it into the hunk contributes nothing;
    `format_rw` marks the row so the total isn't read as complete.
    """
    total = f['ref_time'] + f['reads'] * DB_READ_PS + f['writes'] * DB_WRITE_PS
    for component, max_n in f['ranges'].items():
        total += max_n * (f['coef'].get(component, 0) +
                          f['creads'].get(component, 0) * DB_READ_PS +
                          f['cwrites'].get(component, 0) * DB_WRITE_PS)
    return total


def format_rw(old, new):
    def fmt(a, b):
        return f"{a}→{b}" if a != b else f"{a}"

    def max_n(f, component):
        if component in f['ranges']:
            return str(f['ranges'][component])
        return MAX_UNKNOWN if component in components(f) else '0'

    parts = [f"r {fmt(old['reads'], new['reads'])}",
             f"w {fmt(old['writes'], new['writes'])}"]
    for component in sorted(components(old) | components(new)):
        parts.append(
            f"+{component}≤{fmt(max_n(old, component), max_n(new, component))}×"
            f"(r {fmt(old['creads'].get(component, 0), new['creads'].get(component, 0))}, "
            f"w {fmt(old['cwrites'].get(component, 0), new['cwrites'].get(component, 0))})")
    return ', '.join(parts)


def emit_table(rows):
    print("| | Pallet | Function | Old (ps) | New (ps) | Change (ps) | % | R/W |")
    print("|---|--------|----------|----------|----------|-------------|---|-----|")
    for r in rows:
        indicator = "⚠️" if r['pct'] > 0 else "✅"
        print(f"| {indicator} | {r['pallet']} | {r['function']} | "
              f"{r['old']:,} | {r['new']:,} | {r['diff']:+,} | {r['pct']:+.2f}% | {r['rw']} |")


def emit_details(summary, rows):
    # Blank lines around the table are required for GitHub to render markdown
    # inside a <details> block.
    print(f"<details>\n<summary>{summary}</summary>\n")
    emit_table(rows)
    print("\n</details>\n")


def main():
    diff_content = sys.stdin.read()

    # Split into file sections
    file_sections = re.split(r'diff --git a/(.*?) b/.*?\n', diff_content)

    results = []
    seen = set()

    for i in range(1, len(file_sections), 2):
        if i+1 >= len(file_sections):
            break

        file_path = file_sections[i]
        content = file_sections[i+1]

        # Extract pallet name
        pallet_match = re.search(r'pallets/([\w-]+)/', file_path)
        if not pallet_match:
            continue
        pallet = pallet_match.group(1)

        # Parse each hunk independently: reconstruct the old side (context +
        # removed lines) and the new side (context + added lines), then
        # compare per-function weight data between the two.
        for hunk in re.split(r'^@@.*$', content, flags=re.MULTILINE)[1:]:
            lines = hunk.split('\n')
            old_functions = parse_functions(
                line[1:] for line in lines if line.startswith((' ', '-')))
            new_functions = parse_functions(
                line[1:] for line in lines if line.startswith((' ', '+')))

            for name, new in new_functions.items():
                old = old_functions.get(name)
                if old is None:
                    continue
                old_val = total_ps(old)
                new_val = total_ps(new)
                if old_val == 0 or old == new:
                    continue

                # Each pallet's weights.rs defines every function twice
                # (the `WeightInfo for PalletWeight<T>` impl and the
                # `WeightInfo for ()` fallback impl) with identical values,
                # so dedupe to avoid double-counting each change.
                key = (pallet, name)
                if key in seen:
                    continue
                seen.add(key)
                results.append({
                    'pallet': pallet,
                    'function': name,
                    'old': old_val,
                    'new': new_val,
                    'diff': new_val - old_val,
                    'pct': ((new_val - old_val) / old_val) * 100,
                    'rw': format_rw(old, new),
                })

    if not results:
        print("No weight changes found in input.")
        return

    # Sort by magnitude of percentage change (largest first).
    results.sort(key=lambda x: abs(x['pct']), reverse=True)

    signal = [r for r in results if abs(r['pct']) >= NOISE_PCT]
    noise = [r for r in results if abs(r['pct']) < NOISE_PCT]
    big = [r for r in signal if abs(r['pct']) > BIG_CHANGE_PCT]
    structural = [r for r in results if '→' in r['rw']]
    pallets = {r['pallet'] for r in results}

    # TL;DR headline (always visible).
    if signal:
        up = sum(1 for r in signal if r['pct'] > 0)
        down = sum(1 for r in signal if r['pct'] < 0)
        avg = sum(r['pct'] for r in signal) / len(signal)
        top = signal[0]
        print(
            f"**Weight changes** (worst-case total effective weight, incl. db ops "
            f"and component terms at max): "
            f"{len(results)} functions across {len(pallets)} pallets — "
            f"{up} ↑ / {down} ↓, avg {avg:+.2f}%. "
            f"Largest {top['pct']:+.2f}% `{top['pallet']}::{top['function']}`. "
            f"{len(big)} >{BIG_CHANGE_PCT:.0f}%, {len(noise)} below {NOISE_PCT:.0f}% (noise), "
            f"{len(structural)} with db access or component range changes.\n"
        )
    else:
        print(
            f"**Weight changes** (worst-case total effective weight, incl. db ops "
            f"and component terms at max): "
            f"{len(results)} functions across {len(pallets)} pallets, "
            f"all below the {NOISE_PCT:.0f}% noise floor.\n"
        )

    # Large changes stay expanded — these are what a reviewer must look at.
    if big:
        print(f"### ⚠️ Large changes (>{BIG_CHANGE_PCT:.0f}%)\n")
        emit_table(big[:20])
        print()

    # Db access counts and component ranges are structural (not measurement
    # noise) and always worth a look, however small the percentage impact.
    if structural:
        print("### 🗄️ Db access / component range changes\n")
        emit_table(structural[:20])
        print()

    if signal:
        emit_details(f"Top {TOP_N} by % change", signal[:TOP_N])
        emit_details(
            f"Top {TOP_N} by absolute change (ps)",
            sorted(signal, key=lambda x: abs(x['diff']), reverse=True)[:TOP_N],
        )

        # Per-pallet rollup.
        print("<details>\n<summary>Changes by pallet</summary>\n")
        print("| Pallet | Changes | Avg % | Max % |")
        print("|--------|---------|-------|-------|")
        pallet_stats = {}
        for r in signal:
            s = pallet_stats.setdefault(r['pallet'], {'count': 0, 'sum': 0.0, 'max': 0.0})
            s['count'] += 1
            s['sum'] += r['pct']
            s['max'] = max(s['max'], abs(r['pct']))
        for pallet, s in sorted(pallet_stats.items(), key=lambda x: x[1]['max'], reverse=True):
            print(f"| {pallet} | {s['count']} | {s['sum'] / s['count']:+.2f}% | {s['max']:.2f}% |")
        print("\n</details>\n")

    if noise:
        emit_details(
            f"{len(noise)} changes below {NOISE_PCT:.0f}% (likely measurement noise)",
            noise,
        )

if __name__ == '__main__':
    main()
