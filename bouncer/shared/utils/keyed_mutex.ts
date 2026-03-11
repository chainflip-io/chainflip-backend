import { Mutex, MutexInterface } from 'async-mutex';
import { mutexTracker, getCallerFromStack } from 'shared/utils/mutex_tracker';

export class KeyedMutex {
  private map = new Map<string, Mutex>();

  private readonly name: string;

  constructor(name?: string) {
    this.name = name ?? 'KeyedMutex';
  }

  private get(key: string): Mutex {
    let m = this.map.get(key);
    if (!m) {
      m = new Mutex();
      this.map.set(key, m);
    }
    return m;
  }

  /**
   * Returns a proxy that allows you to call methods on the mutex directly.
   * This is useful for chaining calls like `mutex.for(key).runExclusive(...)`.
   *
   * Note: `runExclusive` calls through the proxy are tracked.
   */
  for(key: string): Mutex {
    const mutex = this.get(key);
    const mutexName = this.name;
    return new Proxy(mutex, {
      get: (target, prop) => {
        if (prop === 'runExclusive') {
          return async <T>(callback: MutexInterface.Worker<T>): Promise<T> => {
            const caller = getCallerFromStack();
            const timestamp = new Date().toISOString();
            const waitStart = Date.now();
            return target.runExclusive(async () => {
              const waitTimeMs = Date.now() - waitStart;
              const holdStart = Date.now();
              try {
                return await callback();
              } finally {
                mutexTracker.record({
                  mutexName,
                  key,
                  waitTimeMs,
                  holdTimeMs: Date.now() - holdStart,
                  caller,
                  timestamp,
                });
              }
            });
          };
        }
        const value = target[prop as keyof Mutex];
        if (typeof value === 'function') {
          return value.bind(target);
        }
        return value;
      },
    });
  }

  async acquire(key: string): Promise<MutexInterface.Releaser> {
    const caller = getCallerFromStack();
    const timestamp = new Date().toISOString();
    const waitStart = Date.now();

    const releaser = await this.get(key).acquire();
    const waitTimeMs = Date.now() - waitStart;
    const holdStart = Date.now();

    return () => {
      mutexTracker.record({
        mutexName: this.name,
        key,
        waitTimeMs,
        holdTimeMs: Date.now() - holdStart,
        caller,
        timestamp,
      });
      releaser();
    };
  }

  async runExclusive<T>(key: string, callback: MutexInterface.Worker<T>): Promise<T> {
    const caller = getCallerFromStack();
    const timestamp = new Date().toISOString();
    const waitStart = Date.now();

    return this.get(key).runExclusive(async () => {
      const waitTimeMs = Date.now() - waitStart;
      const holdStart = Date.now();
      try {
        return await callback();
      } finally {
        mutexTracker.record({
          mutexName: this.name,
          key,
          waitTimeMs,
          holdTimeMs: Date.now() - holdStart,
          caller,
          timestamp,
        });
      }
    });
  }

  isLocked(key: string): boolean {
    return this.get(key).isLocked();
  }

  waitForUnlock(key: string): Promise<void> {
    return this.get(key).waitForUnlock();
  }

  cancel(key: string): void {
    return this.get(key).cancel();
  }

  release(key: string): void {
    return this.get(key).release();
  }
}
