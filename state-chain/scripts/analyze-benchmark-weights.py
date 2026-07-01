#!/usr/bin/env python3
"""
Analysis of benchmark weight changes, showing function-level changes.

Usage:
    git diff -U50 main...HEAD state-chain/pallets/*/src/weights.rs | ./state-chain/scripts/analyze-benchmark-weights.py

This script provides function-level analysis of benchmark weight changes,
including which pallets and functions have the most significant changes.
"""

import re
import sys

# Output tuning. A per-function change below NOISE_PCT is treated as benchmark
# measurement noise and folded into a collapsed section rather than the headline.
TOP_N = 30
BIG_CHANGE_PCT = 50.0
NOISE_PCT = 2.0


def emit_table(rows):
    print("| | Pallet | Function | Old (ps) | New (ps) | Change (ps) | % |")
    print("|---|--------|----------|----------|----------|-------------|---|")
    for r in rows:
        indicator = "⚠️" if r['pct'] > 0 else "✅"
        print(f"| {indicator} | {r['pallet']} | {r['function']} | "
              f"{r['old']:,} | {r['new']:,} | {r['diff']:+,} | {r['pct']:+.2f}% |")


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

        # Find function names and their weight changes
        lines = content.split('\n')

        for j, line in enumerate(lines):
            # Look for weight changes
            if '// Minimum execution time:' in line and line.startswith('-'):
                old_match = re.search(r'([\d_]+) picoseconds', line)
                # Find corresponding new line
                for k in range(j+1, min(j+5, len(lines))):
                    if '// Minimum execution time:' in lines[k] and lines[k].startswith('+'):
                        new_match = re.search(r'([\d_]+) picoseconds', lines[k])
                        if old_match and new_match:
                            old_val = int(old_match.group(1).replace('_', ''))
                            new_val = int(new_match.group(1).replace('_', ''))
                            if old_val > 0:
                                # Search backwards to find function name
                                function_name = 'unknown'
                                for back_idx in range(j-1, max(0, j-50), -1):
                                    back_line = lines[back_idx]
                                    # Look for function definition (can be on any line type: context, +, or -)
                                    fn_match = re.search(r'fn\s+(\w+)\s*\(', back_line)
                                    if fn_match:
                                        function_name = fn_match.group(1)
                                        break

                                # Each pallet's weights.rs defines every function twice
                                # (the `WeightInfo for PalletWeight<T>` impl and the
                                # `WeightInfo for ()` fallback impl) with identical values,
                                # so dedupe to avoid double-counting each change.
                                key = (pallet, function_name)
                                if key not in seen:
                                    seen.add(key)
                                    pct_change = ((new_val - old_val) / old_val) * 100
                                    results.append({
                                        'pallet': pallet,
                                        'function': function_name,
                                        'old': old_val,
                                        'new': new_val,
                                        'diff': new_val - old_val,
                                        'pct': pct_change
                                    })
                        break

    if not results:
        print("No weight changes found in input.")
        return

    # Sort by magnitude of percentage change (largest first).
    results.sort(key=lambda x: abs(x['pct']), reverse=True)

    signal = [r for r in results if abs(r['pct']) >= NOISE_PCT]
    noise = [r for r in results if abs(r['pct']) < NOISE_PCT]
    big = [r for r in signal if abs(r['pct']) > BIG_CHANGE_PCT]
    pallets = {r['pallet'] for r in results}

    # TL;DR headline (always visible).
    if signal:
        up = sum(1 for r in signal if r['pct'] > 0)
        down = sum(1 for r in signal if r['pct'] < 0)
        avg = sum(r['pct'] for r in signal) / len(signal)
        top = signal[0]
        print(
            f"**Weight changes:** {len(results)} functions across {len(pallets)} pallets — "
            f"{up} ↑ / {down} ↓, avg {avg:+.2f}%. "
            f"Largest {top['pct']:+.2f}% `{top['pallet']}::{top['function']}`. "
            f"{len(big)} >{BIG_CHANGE_PCT:.0f}%, {len(noise)} below {NOISE_PCT:.0f}% (noise).\n"
        )
    else:
        print(
            f"**Weight changes:** {len(results)} functions across {len(pallets)} pallets, "
            f"all below the {NOISE_PCT:.0f}% noise floor.\n"
        )

    # Large changes stay expanded — these are what a reviewer must look at.
    if big:
        print(f"### ⚠️ Large changes (>{BIG_CHANGE_PCT:.0f}%)\n")
        emit_table(big[:20])
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
