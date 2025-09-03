export class AsyncQueue<T> {
  private queue: T[] = [];

  private pendingResolvers: ((value: T) => void)[] = [];

  private ended = false;

  push(item: T) {
    if (this.pendingResolvers.length) {
      const resolve = this.pendingResolvers.shift()!;
      resolve(item);
    } else {
      this.queue.push(item);
    }
  }

  end() {
    this.ended = true;
    this.pendingResolvers.forEach((resolve) => resolve(null as unknown as T));
    this.pendingResolvers = [];
  }

  async next(): Promise<IteratorResult<T>> {
    if (this.queue.length) {
      return { value: this.queue.shift()!, done: false };
    }
    if (this.ended) {
      return { value: undefined as unknown as T, done: true };
    }
    return new Promise((resolve) => {
      this.pendingResolvers.push((value) => {
        // End of stream: by convention, if a null is pushed, we end.
        if (value === null) {
          resolve({ value: undefined as unknown as T, done: true });
        } else {
          resolve({ value, done: false });
        }
      });
    });
  }

  async *[Symbol.asyncIterator](): AsyncGenerator<T> {
    while (true) {
      const { value, done } = await this.next();
      if (done) break;
      yield value;
    }
  }
}
