import { Mutex, MutexInterface } from 'async-mutex';

const WAIT_WARNING_INTERVAL_MS = 20_000;

export class KeyedMutex {
  private map = new Map<string, Mutex>();

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
   */
  for(key: string): Mutex {
    const mutex = this.get(key);
    // eslint-disable-next-line @typescript-eslint/no-this-alias
    const self = this;
    return new Proxy(mutex, {
      get: (target, prop) => {
        if (prop === 'acquire') {
          return () => self.acquire(key);
        }
        if (prop === 'runExclusive') {
          return <T>(callback: MutexInterface.Worker<T>) => self.runExclusive(key, callback);
        }
        const value = target[prop as keyof Mutex];
        if (typeof value === 'function') {
          return value.bind(target);
        }
        return value;
      },
    });
  }

  acquire(key: string): Promise<MutexInterface.Releaser> {
    const mutex = this.get(key);
    if (!mutex.isLocked()) {
      return mutex.acquire();
    }
    const timer = setInterval(() => {
      console.warn(`Still waiting for lock "${key}"...`);
    }, WAIT_WARNING_INTERVAL_MS);
    return mutex.acquire().finally(() => clearInterval(timer));
  }

  runExclusive<T>(key: string, callback: MutexInterface.Worker<T>): Promise<T> {
    const mutex = this.get(key);
    if (!mutex.isLocked()) {
      return mutex.runExclusive(callback);
    }
    const timer = setInterval(() => {
      console.warn(`Still waiting for lock "${key}"...`);
    }, WAIT_WARNING_INTERVAL_MS);
    return mutex
      .runExclusive(() => {
        clearInterval(timer);
        return callback();
      })
      .finally(() => clearInterval(timer));
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
