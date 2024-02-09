/* eslint-disable max-classes-per-file */
class Timer<T> {
  timeout?: ReturnType<typeof setTimeout>;

  constructor(
    readonly value: T,
    readonly cb: () => void,
    readonly ms: number,
  ) {
    this.reset();
  }

  reset(): this {
    this.clear();
    this.timeout = setTimeout(this.cb, this.ms);
    return this;
  }

  getValue(): T {
    return this.reset().value;
  }

  clear(): void {
    if (this.timeout) clearTimeout(this.timeout);
  }
}

export class CacheMap<K, V> {
  private readonly store = new Map<K, Timer<V>>();

  constructor(private readonly ttl: number) {}

  set(key: K, value: V): this {
    this.delete(key);

    const timer = new Timer(value, () => this.store.delete(key), this.ttl);

    this.store.set(key, timer);
    return this;
  }

  get(key: K): V | undefined {
    return this.store.get(key)?.getValue();
  }

  delete(key: K): boolean {
    this.store.get(key)?.clear();
    return this.store.delete(key);
  }
}
