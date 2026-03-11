import { Semaphore, SemaphoreInterface } from 'async-mutex';
import { mutexTracker, getCallerFromStack } from 'shared/utils/mutex_tracker';

/**
 * A drop-in replacement for `Semaphore` from `async-mutex` that records
 * wait and hold durations to the global MutexTracker.
 */
export class TrackedSemaphore {
  private readonly semaphore: Semaphore;

  private readonly name: string;

  constructor(name: string, maxConcurrency: number) {
    this.name = name;
    this.semaphore = new Semaphore(maxConcurrency);
  }

  async runExclusive<T>(callback: SemaphoreInterface.Worker<T>): Promise<T> {
    const caller = getCallerFromStack();
    const timestamp = new Date().toISOString();
    const waitStart = Date.now();

    return this.semaphore.runExclusive(async (value) => {
      const waitTimeMs = Date.now() - waitStart;
      const holdStart = Date.now();
      try {
        return await callback(value);
      } finally {
        mutexTracker.record({
          kind: 'semaphore',
          mutexName: this.name,
          waitTimeMs,
          holdTimeMs: Date.now() - holdStart,
          caller,
          timestamp,
        });
      }
    });
  }

  async acquire(): Promise<[number, SemaphoreInterface.Releaser]> {
    const caller = getCallerFromStack();
    const timestamp = new Date().toISOString();
    const waitStart = Date.now();

    const [value, releaser] = await this.semaphore.acquire();
    const waitTimeMs = Date.now() - waitStart;
    const holdStart = Date.now();

    const trackedReleaser: SemaphoreInterface.Releaser = () => {
      mutexTracker.record({
        kind: 'semaphore',
        mutexName: this.name,
        waitTimeMs,
        holdTimeMs: Date.now() - holdStart,
        caller,
        timestamp,
      });
      releaser();
    };

    return [value, trackedReleaser];
  }

  getValue(): number {
    return this.semaphore.getValue();
  }

  isLocked(): boolean {
    return this.semaphore.isLocked();
  }

  waitForUnlock(): Promise<void> {
    return this.semaphore.waitForUnlock();
  }

  cancel(): void {
    return this.semaphore.cancel();
  }

  release(): void {
    return this.semaphore.release();
  }
}
