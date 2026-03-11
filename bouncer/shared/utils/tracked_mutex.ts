import { Mutex, MutexInterface } from 'async-mutex';
import { mutexTracker, getCallerFromStack } from 'shared/utils/mutex_tracker';

/**
 * A drop-in replacement for `Mutex` from `async-mutex` that records
 * wait and hold durations to the global MutexTracker.
 */
export class TrackedMutex {
  private readonly mutex = new Mutex();

  private readonly name: string;

  private readonly key?: string;

  constructor(name: string, key?: string) {
    this.name = name;
    this.key = key;
  }

  async runExclusive<T>(callback: MutexInterface.Worker<T>): Promise<T> {
    const caller = getCallerFromStack();
    const timestamp = new Date().toISOString();
    const waitStart = Date.now();

    return this.mutex.runExclusive(async () => {
      const waitTimeMs = Date.now() - waitStart;
      const holdStart = Date.now();
      try {
        return await callback();
      } finally {
        mutexTracker.record({
          mutexName: this.name,
          key: this.key,
          waitTimeMs,
          holdTimeMs: Date.now() - holdStart,
          caller,
          timestamp,
        });
      }
    });
  }

  async acquire(): Promise<MutexInterface.Releaser> {
    const caller = getCallerFromStack();
    const timestamp = new Date().toISOString();
    const waitStart = Date.now();

    const releaser = await this.mutex.acquire();
    const waitTimeMs = Date.now() - waitStart;
    const holdStart = Date.now();

    return () => {
      mutexTracker.record({
        mutexName: this.name,
        key: this.key,
        waitTimeMs,
        holdTimeMs: Date.now() - holdStart,
        caller,
        timestamp,
      });
      releaser();
    };
  }

  isLocked(): boolean {
    return this.mutex.isLocked();
  }

  waitForUnlock(): Promise<void> {
    return this.mutex.waitForUnlock();
  }

  cancel(): void {
    return this.mutex.cancel();
  }

  release(): void {
    return this.mutex.release();
  }
}
