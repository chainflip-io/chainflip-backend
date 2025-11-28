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

def main():
    diff_content = sys.stdin.read()

    # Split into file sections
    file_sections = re.split(r'diff --git a/(.*?) b/.*?\n', diff_content)

    results = []

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

    # Sort by absolute percentage change
    results.sort(key=lambda x: abs(x['pct']), reverse=True)

    print("TOP 30 MOST SIGNIFICANT FUNCTION WEIGHT CHANGES:")
    print("=" * 120)
    print(f"{'Pallet':<25} {'Function':<30} {'Old (ps)':>15} {'New (ps)':>15} {'% Change':>10}")
    print("-" * 120)

    for item in results[:30]:
        print(f"{item['pallet']:<25} {item['function']:<30} {item['old']:>15,} {item['new']:>15,} {item['pct']:>+9.2f}%")

    # Group by pallet
    print("\n\nCHANGES BY PALLET:")
    print("=" * 120)

    pallet_stats = {}
    for r in results:
        pallet = r['pallet']
        if pallet not in pallet_stats:
            pallet_stats[pallet] = {'count': 0, 'avg_pct': 0, 'max_pct': 0, 'functions': []}
        pallet_stats[pallet]['count'] += 1
        pallet_stats[pallet]['avg_pct'] += r['pct']
        pallet_stats[pallet]['max_pct'] = max(pallet_stats[pallet]['max_pct'], abs(r['pct']))
        pallet_stats[pallet]['functions'].append(r['function'])

    for pallet, stats in sorted(pallet_stats.items(), key=lambda x: x[1]['max_pct'], reverse=True):
        avg = stats['avg_pct'] / stats['count']
        print(f"{pallet:<25} Changes: {stats['count']:>3}   Avg: {avg:>+7.2f}%   Max: {stats['max_pct']:>7.2f}%")

    # Show very significant changes
    print("\n\nCHANGES > 50%:")
    print("=" * 120)
    big_changes = [r for r in results if abs(r['pct']) > 50]
    print(f"Found {len(big_changes)} functions with >50% change\n")

    for item in big_changes[:20]:
        indicator = "⚠️ " if item['pct'] > 0 else "✅"
        print(f"{indicator} {item['pallet']:<25} {item['function']:<30} {item['old']:>15,} {item['new']:>15,} {item['pct']:>+9.2f}%")

if __name__ == '__main__':
    main()
