import { mkdirSync, writeFileSync } from 'fs';
import { dirname } from 'path';

export type RecordKind = 'mutex' | 'semaphore';

export interface MutexRecord {
  kind: RecordKind;
  mutexName: string;
  key?: string;
  waitTimeMs: number;
  holdTimeMs: number;
  caller: string;
  timestamp: string;
}

const WAIT_TIME_THRESHOLD_MS = 15_000;
const HOLD_TIME_THRESHOLD_MS = 15_000;

const REPORT_DIR = '/tmp/chainflip';

// Build a unique report filename per vitest process.
// Includes the suite name from `-t` if present, plus a timestamp for uniqueness.
function buildReportPath(): string {
  const tIndex = process.argv.indexOf('-t');
  const suiteName =
    tIndex !== -1 && tIndex + 1 < process.argv.length
      ? `_${process.argv[tIndex + 1].replace(/[^a-zA-Z0-9_-]/g, '_')}`
      : '';
  const ts = new Date().toISOString().replace(/[:.]/g, '-');
  return `${REPORT_DIR}/mutex_report${suiteName}_${ts}.md`;
}

const globalReportPath = buildReportPath();

function formatTable(title: string, records: MutexRecord[]): string {
  let md = `## ${title}\n\n`;
  if (records.length === 0) {
    md += `No significant contention detected (threshold: 15s wait time).\n\n`;
  } else {
    md += '| Name | Key | Wait (s) | Hold (s) | Caller | Time |\n';
    md += '|---|---|---|---|---|---|\n';
    for (const r of records) {
      const wait = (r.waitTimeMs / 1000).toFixed(1);
      const hold = (r.holdTimeMs / 1000).toFixed(1);
      const key = r.key ?? '—';
      md += `| ${r.mutexName} | ${key} | ${wait} | ${hold} | ${r.caller} | ${r.timestamp} |\n`;
    }
    md += '\n';
  }
  return md;
}

class MutexTrackerSingleton {
  private records: MutexRecord[] = [];

  record(entry: MutexRecord): void {
    if (entry.waitTimeMs >= WAIT_TIME_THRESHOLD_MS || entry.holdTimeMs >= HOLD_TIME_THRESHOLD_MS) {
      this.records.push(entry);
    }
  }

  getRecords(): MutexRecord[] {
    return [...this.records].sort((a, b) => b.waitTimeMs - a.waitTimeMs);
  }

  writeReportFile(): void {
    const sorted = this.getRecords();
    const mutexRecords = sorted.filter((r) => r.kind === 'mutex');
    const semaphoreRecords = sorted.filter((r) => r.kind === 'semaphore');

    let md = '';
    md += formatTable('Mutex Contention Report', mutexRecords);
    md += formatTable('Semaphore Contention Report', semaphoreRecords);

    mkdirSync(dirname(globalReportPath), { recursive: true });
    writeFileSync(globalReportPath, md);
  }
}

export const mutexTracker = new MutexTrackerSingleton();

/**
 * Extract a short caller description from a stack trace.
 * Skips internal frames (mutex_tracker, keyed_mutex, tracked_mutex, async-mutex).
 */
export function getCallerFromStack(): string {
  const err = new Error();
  const lines = (err.stack ?? '').split('\n');
  for (const line of lines) {
    // Skip the Error line, and frames from our own instrumentation
    if (
      line.includes('mutex_tracker') ||
      line.includes('keyed_mutex') ||
      line.includes('tracked_mutex') ||
      line.includes('tracked_semaphore') ||
      line.includes('async-mutex') ||
      line.includes('node_modules') ||
      line.includes('node:') ||
      line.startsWith('Error')
    ) {
      // eslint-disable-next-line no-continue
      continue;
    }
    // Extract filename and line number from the stack frame.
    // Vitest may strip .ts extensions from aliased imports, so match with or without extension.
    const match = line.match(/([^/\s(]+):([0-9]+):[0-9]+/);
    if (match) {
      return `${match[1]}:${match[2]}`;
    }
  }
  return 'unknown';
}
