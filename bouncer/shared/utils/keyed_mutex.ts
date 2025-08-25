import { Mutex, MutexInterface } from 'async-mutex';

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
    return new Proxy(mutex, {
      get: (target, prop) => {
        const value = target[prop as keyof Mutex];
        if (typeof value === 'function') {
          return value.bind(target);
        }
        return value;
      },
    });
  }

  acquire(key: string): Promise<MutexInterface.Releaser> {
    return this.get(key).acquire();
  }

  runExclusive<T>(key: string, callback: MutexInterface.Worker<T>): Promise<T> {
    return this.get(key).runExclusive(callback);
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
