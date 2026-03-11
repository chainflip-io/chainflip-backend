import { mkdirSync, writeFileSync } from 'fs';
import { dirname } from 'path';

export interface MutexRecord {
  mutexName: string;
  key?: string;
  waitTimeMs: number;
  holdTimeMs: number;
  caller: string;
  timestamp: string;
}

const WAIT_TIME_THRESHOLD_MS = 15_000;

const REPORT_FILE_PATH = '/tmp/chainflip/mutex_report.md';

class MutexTrackerSingleton {
  private records: MutexRecord[] = [];

  record(entry: MutexRecord): void {
    if (entry.waitTimeMs >= WAIT_TIME_THRESHOLD_MS) {
      this.records.push(entry);
    }
  }

  getRecords(): MutexRecord[] {
    return [...this.records].sort((a, b) => b.waitTimeMs - a.waitTimeMs);
  }

  writeReportFile(path: string = REPORT_FILE_PATH): void {
    const sorted = this.getRecords();

    let md = '## Mutex Contention Report\n\n';
    if (sorted.length === 0) {
      md += 'No significant mutex contention detected (threshold: 15s wait time).\n';
    } else {
      md += '| Mutex | Key | Wait (s) | Hold (s) | Caller | Time |\n';
      md += '|---|---|---|---|---|---|\n';
      for (const r of sorted) {
        const wait = (r.waitTimeMs / 1000).toFixed(1);
        const hold = (r.holdTimeMs / 1000).toFixed(1);
        const key = r.key ?? '—';
        md += `| ${r.mutexName} | ${key} | ${wait} | ${hold} | ${r.caller} | ${r.timestamp} |\n`;
      }
    }

    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, md);
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
      line.includes('async-mutex') ||
      line.startsWith('Error')
    ) {
      // eslint-disable-next-line no-continue
      continue;
    }
    // Extract filename and line number from the stack frame
    const match = line.match(/([^/\s]+\.ts):(\d+):\d+/);
    if (match) {
      return `${match[1]}:${match[2]}`;
    }
  }
  return 'unknown';
}
