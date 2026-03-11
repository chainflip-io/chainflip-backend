import { Mutex, MutexInterface } from 'async-mutex';
import { TrackedMutex } from 'shared/utils/tracked_mutex';

export class KeyedMutex {
  private map = new Map<string, TrackedMutex>();

  private readonly name: string;

  constructor(name?: string) {
    this.name = name ?? 'KeyedMutex';
  }

  private get(key: string): TrackedMutex {
    let m = this.map.get(key);
    if (!m) {
      m = new TrackedMutex(this.name, key);
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
    const tracked = this.get(key);
    return new Proxy(tracked as unknown as Mutex, {
      get: (_target, prop) => {
        const value = tracked[prop as keyof TrackedMutex];
        if (typeof value === 'function') {
          return value.bind(tracked);
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
