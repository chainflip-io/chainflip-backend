import { CacheMap } from '../dataStructures';

describe(CacheMap, () => {
  beforeEach(() => {
    jest.useFakeTimers();
  });

  afterEach(() => {
    jest.useRealTimers();
  });

  it('caches a value and resets the timer on access', () => {
    const setTimeoutSpy = jest.spyOn(globalThis, 'setTimeout');
    const clearTimeoutSpy = jest.spyOn(globalThis, 'clearTimeout');

    const map = new CacheMap<string, string>(10);
    map.set('hello', 'world');

    jest.advanceTimersByTime(9);
    expect(map.get('hello')).toBe('world');

    expect(setTimeoutSpy).toHaveBeenCalledTimes(2);
    expect(clearTimeoutSpy).toHaveBeenCalledTimes(1);

    jest.advanceTimersByTime(9);
    expect(map.get('hello')).toBe('world');

    expect(setTimeoutSpy).toHaveBeenCalledTimes(3);
    expect(clearTimeoutSpy).toHaveBeenCalledTimes(2);
  });

  it('deletes values and clears timeouts', () => {
    const map = new CacheMap<string, string>(10);
    const spy = jest.spyOn(globalThis, 'clearTimeout');
    map.set('hello', 'world');
    map.delete('hello');
    expect(spy).toHaveBeenCalledTimes(1);
    expect(map.get('hello')).toBe(undefined);
  });

  it('expires values properly', () => {
    const setTimeoutSpy = jest.spyOn(globalThis, 'setTimeout');
    const clearTimeoutSpy = jest.spyOn(globalThis, 'clearTimeout');

    const map = new CacheMap<string, string>(10);
    map.set('hello', 'world');

    expect(setTimeoutSpy).toHaveBeenCalledTimes(1);

    jest.advanceTimersByTime(10);
    expect(map.get('hello')).toBe(undefined);

    expect(setTimeoutSpy).toHaveBeenCalledTimes(1);
    expect(clearTimeoutSpy).not.toHaveBeenCalled();
  });
});
