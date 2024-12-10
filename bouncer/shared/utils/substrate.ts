import 'disposablestack/auto';
import { ApiPromise, WsProvider } from '@polkadot/api';
import { Observable, Subject } from 'rxjs';
import { deferredPromise, sleep } from '../utils';

// @ts-expect-error polyfilling
Symbol.asyncDispose ??= Symbol('asyncDispose');
// @ts-expect-error polyfilling
Symbol.dispose ??= Symbol('dispose');

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const getCachedDisposable = <T extends AsyncDisposable, F extends (...args: any[]) => Promise<T>>(
  factory: F,
) => {
  const cache = new Map<string, Promise<T>>();
  let connections = 0;

  return (async (...args) => {
    const cacheKey = JSON.stringify(args);
    let disposablePromise = cache.get(cacheKey);

    if (!disposablePromise) {
      disposablePromise = factory(...args);
      cache.set(cacheKey, disposablePromise);
    }

    const disposable = await disposablePromise;

    connections += 1;

    return new Proxy(disposable, {
      get(target, prop, receiver) {
        if (prop === Symbol.asyncDispose) {
          return () => {
            connections -= 1;
            setTimeout(() => {
              if (connections === 0) {
                const dispose = Reflect.get(
                  target,
                  Symbol.asyncDispose,
                  receiver,
                ) as unknown as () => Promise<void>;

                dispose.call(target).catch(() => null);
                cache.delete(cacheKey);
              }
            }, 5_000).unref();
          };
        }

        return Reflect.get(target, prop, receiver);
      },
    });
  }) as F;
};

type DisposableApiPromise = ApiPromise & { [Symbol.asyncDispose](): Promise<void> };

// It is important to cache WS connections because nodes seem to have a
// limit on how many can be opened at the same time (from the same IP presumably)
const getCachedSubstrateApi = (endpoint: string) =>
  getCachedDisposable(async (): Promise<DisposableApiPromise> => {
    const apiPromise = await ApiPromise.create({
      provider: new WsProvider(endpoint),
      noInitWarn: true,
      types: {
        EncodedAddress: {
          _enum: {
            Eth: '[u8; 20]',
            Arb: '[u8; 20]',
            Dot: '[u8; 32]',
            Btc: 'Vec<u8>',
          },
        },
      },
    });

    return new Proxy(apiPromise as unknown as DisposableApiPromise, {
      get(target, prop, receiver) {
        if (prop === Symbol.asyncDispose) {
          return Reflect.get(target, 'disconnect', receiver);
        }
        if (prop === 'disconnect') {
          return async () => {
            // noop
          };
        }

        return Reflect.get(target, prop, receiver);
      },
    });
  });

export const getChainflipApi = getCachedSubstrateApi(
  process.env.CF_NODE_ENDPOINT ?? 'ws://127.0.0.1:9944',
);
export const getPolkadotApi = getCachedSubstrateApi(
  process.env.POLKADOT_ENDPOINT ?? 'ws://127.0.0.1:9947',
);

const apiMap = {
  chainflip: getChainflipApi,
  polkadot: getPolkadotApi,
} as const;

type SubstrateChain = keyof typeof apiMap;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type Event<T = any> = {
  name: { section: string; method: string };
  data: T;
  block: number;
  eventIndex: number;
};

// Iterate over all the events, reshaping them for the consumer
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function mapEvents(events: any[], blockNumber: number): Event[] {
  return events.map(({ event }, index) => ({
    name: { section: event.section, method: event.method },
    data: event.toHuman().data,
    block: blockNumber,
    eventIndex: index,
  }));
}

class EventCache {
  private cache: Map<string, Event[]>;

  private blockNumberToHash: Map<number, string>;

  private cacheSizeBlocks: number;

  private chain: SubstrateChain;

  private finalisedBlockNumber: number | undefined;

  private pendingRequests: number[];

  constructor(cacheSizeBlocks: number, chain: SubstrateChain) {
    this.cache = new Map();
    this.blockNumberToHash = new Map();
    this.cacheSizeBlocks = cacheSizeBlocks;
    this.chain = chain;
    this.finalisedBlockNumber = undefined;
    this.pendingRequests = [];
  }

  addEvents(blockHash: string, events: Event[], isFinalised = false) {
    if (this.cache.has(blockHash)) {
      return; // Event already in cache
    }
    // Removed old block with different hash
    const blockNumber = events[0].block;
    if (this.blockNumberToHash.has(blockNumber)) {
      this.cache.delete(this.blockNumberToHash.get(blockNumber)!);
      this.blockNumberToHash.delete(blockNumber);
    }

    // Add new block to cache
    this.cache.set(blockHash, events);
    this.blockNumberToHash.set(blockNumber, blockHash);
    if (isFinalised) {
      this.finalisedBlockNumber = blockNumber;
    }

    // Remove old blocks to maintain cache size
    if (this.cache.size > this.cacheSizeBlocks) {
      const oldestBlockNumber = Array.from(this.blockNumberToHash.keys()).reduce(
        (oldest, current) => (current < oldest! ? current : oldest),
      );
      this.cache.delete(this.blockNumberToHash.get(oldestBlockNumber)!);
      this.blockNumberToHash.delete(oldestBlockNumber);
    }
  }

  async getEvents(fromBlock: number, toBlock: number): Promise<Event[]> {
    const api = await apiMap[this.chain]();
    const events: Event[] = [];

    // Update the finalised block number if needed
    if (this.finalisedBlockNumber === undefined || this.finalisedBlockNumber! < toBlock) {
      const newFinalisedHash = await api.rpc.chain.getFinalizedHead();
      this.finalisedBlockNumber = (
        await api.rpc.chain.getHeader(newFinalisedHash)
      ).number.toNumber();
    }

    // Get events from cache or fetch them from the node
    for (let i = fromBlock; i <= toBlock; i++) {
      let blockHash = this.blockNumberToHash.get(i);
      // No need to fetch the hash if it is already finalised and in the cache
      if (
        blockHash === undefined ||
        this.finalisedBlockNumber === undefined ||
        i > this.finalisedBlockNumber!
      ) {
        blockHash = (await api.rpc.chain.getBlockHash(i)).toString();
      }
      // Wait for the block to be fetched if someone else already requested it
      let timeout = 0;
      while (this.pendingRequests.includes(i)) {
        if (timeout++ > 50) {
          break;
        }
        await sleep(100);
      }
      if (this.cache.has(blockHash)) {
        events.push(...this.cache.get(blockHash)!);
      } else {
        this.pendingRequests.push(i);
        const historicApi = await api.at(blockHash);
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const rawEvents = (await historicApi.query.system.events()) as unknown as any[];
        const mappedEvents = mapEvents(rawEvents, i);
        this.addEvents(blockHash, mappedEvents);
        this.pendingRequests = this.pendingRequests.filter((block) => block !== i);
        events.push(...mappedEvents);
      }
    }
    return events;
  }
}

const chainflipEventCache = new EventCache(100, 'chainflip');
const polkadotEventCache = new EventCache(100, 'polkadot');
const eventCacheMap = {
  chainflip: chainflipEventCache,
  polkadot: polkadotEventCache,
} as const;

async function* observableToIterable<T>(observer: Observable<T>, signal?: AbortSignal) {
  // async generator is pull-based, but the observable is push-based
  // if the consumer takes too long, we need to buffer the events
  const buffer: T[] = [];

  // yield the first batch of events via a promise because it is asynchronous
  let promise: Promise<T | null> | undefined;
  let resolve: ((value: T | null) => void) | undefined;
  let reject: ((error: Error) => void) | undefined;
  ({ resolve, promise, reject } = deferredPromise<T | null>());
  let done = false;

  const complete = () => {
    done = true;
    resolve?.(null);
  };

  signal?.addEventListener('abort', complete, { once: true });

  const sub = observer.subscribe({
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    error: (error: any) => {
      reject?.(error);
    },
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    next: (value: any) => {
      // if we haven't consumed the promise yet, resolve it and prepare the for
      // the next batch, otherwise begin buffering the events
      if (resolve) {
        resolve(value);
        promise = undefined;
        resolve = undefined;
        reject = undefined;
      } else {
        buffer.push(value);
      }
    },
    complete,
  });

  while (!done) {
    const next = await promise.catch(() => null);

    // yield the first batch
    if (next === null) break;
    yield next;

    // if the consume took too long, yield the buffered events
    while (buffer.length !== 0) {
      yield buffer.shift()!;
    }

    // reset for the next batch
    ({ resolve, promise, reject } = deferredPromise<T | null>());
  }

  sub.unsubscribe();
}

// Get the total weight of the latest block in percentage
async function getBlockWeight(api: DisposableApiPromise): Promise<number> {
  const maxWeight = 2_000_000_000_000;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const blockWeightInfo = (await api.query.system.blockWeight()) as any;
  const totalWeight =
    Number(blockWeightInfo.normal.refTime) +
    Number(blockWeightInfo.operational.refTime) +
    Number(blockWeightInfo.mandatory.refTime);
  return Math.round((totalWeight / maxWeight) * 100);
}

const subscribeHeads = getCachedDisposable(
  async ({ chain, finalized = false }: { chain: SubstrateChain; finalized?: boolean }) => {
    // prepare a stack for cleanup
    const stack = new AsyncDisposableStack();
    // Take the correct substrate API
    const api = stack.use(await apiMap[chain]());

    const subject = new Subject<Event[]>();

    // subscribe to the correct head based on the finalized flag
    const subscribe = finalized
      ? api.rpc.chain.subscribeFinalizedHeads
      : api.rpc.chain.subscribeNewHeads;

    const unsubscribe = await subscribe(async (header) => {
      const historicApi = await api.at(header.hash);

      // Log block weight to help with debugging
      if (chain === 'chainflip') {
        const weightPercent = await getBlockWeight(historicApi as DisposableApiPromise);
        const message = `Block weight (${header.number}${finalized ? '' : '*'}): ${weightPercent}%`;
        if (weightPercent < 90) {
          console.log(message);
        } else {
          console.warn(message);
        }
      }

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const rawEvents = (await historicApi.query.system.events()) as unknown as any[];
      const mappedEvents = mapEvents(rawEvents, header.number.toNumber());
      const cache = eventCacheMap[chain];
      cache.addEvents(header.hash.toString(), mappedEvents, finalized);
      subject.next(mappedEvents);
    });

    // automatic cleanup!
    stack.defer(unsubscribe);
    stack.defer(() => subject.complete());

    return {
      observable: subject as Observable<Event[]>,
      [Symbol.asyncDispose]() {
        return stack.disposeAsync();
      },
    };
  },
);

async function getPastEvents(
  chain: SubstrateChain,
  historicalCheckBlocks: number,
): Promise<Event[]> {
  if (historicalCheckBlocks <= 0) {
    throw new Error('Invalid Historical Check Blocks');
  }
  const api = await apiMap[chain]();
  const latestHeader = await api.rpc.chain.getHeader();
  const latestBlockNumber = latestHeader.number.toNumber();
  const startAtBlock = Math.max(latestBlockNumber - historicalCheckBlocks, 0);
  const cache = eventCacheMap[chain];
  return cache.getEvents(startAtBlock, latestBlockNumber);
}

type EventTest<T> = (event: Event<T>) => boolean;

interface BaseOptions<T> {
  chain?: SubstrateChain;
  test?: EventTest<T>;
  finalized?: boolean;
  historicalCheckBlocks?: number;
}

interface Options<T> extends BaseOptions<T> {
  abortable?: false;
}

interface AbortableOptions<T> extends BaseOptions<T> {
  abortable: true;
}

type EventName = `${string}:${string}`;

type Observer<T> = {
  events: Promise<Event<T>[]>;
};

type AbortableObserver<T> = {
  stop: () => void;
  events: Promise<Event<T>[] | null>;
};

/* eslint-disable @typescript-eslint/no-explicit-any */
export function observeEvents<T = any>(eventName: EventName): Observer<T>;
export function observeEvents<T = any>(eventName: EventName, opts: Options<T>): Observer<T>;
export function observeEvents<T = any>(
  eventName: EventName,
  opts: AbortableOptions<T>,
): AbortableObserver<T>;
export function observeEvents<T = any>(
  eventName: EventName,
  {
    chain = 'chainflip',
    test = () => true,
    finalized = false,
    historicalCheckBlocks = 0,
    abortable = false,
  }: Options<T> | AbortableOptions<T> = {},
) {
  const [expectedSection, expectedMethod] = eventName.split(':');

  const controller = abortable ? new AbortController() : undefined;

  const findEvent = async () => {
    const foundEvents: Event[] = [];

    // Check historic events first
    if (historicalCheckBlocks > 0) {
      const historicEvents = await getPastEvents(chain, historicalCheckBlocks);
      for (const event of historicEvents) {
        if (
          event.name.section.includes(expectedSection) &&
          event.name.method.includes(expectedMethod) &&
          test(event)
        ) {
          foundEvents.push(event);
        }
      }
    }
    if (foundEvents.length > 0) {
      // No need to continue if we found event(s) in the past
      return foundEvents;
    }

    // Subscribe to new events and wait for the first match
    await using subscription = await subscribeHeads({ chain, finalized });
    const subscriptionIterator = observableToIterable(subscription.observable, controller?.signal);
    for await (const events of subscriptionIterator) {
      for (const event of events) {
        if (
          event.name.section.includes(expectedSection) &&
          event.name.method.includes(expectedMethod) &&
          test(event)
        ) {
          foundEvents.push(event);
        }
      }
      if (foundEvents.length > 0) {
        return foundEvents;
      }
    }

    return null;
  };

  if (!controller) return { events: findEvent() } as Observer<T>;

  return { stop: () => controller.abort(), events: findEvent() } as AbortableObserver<T>;
}

type SingleEventAbortableObserver<T> = {
  stop: () => void;
  event: Promise<Event<T> | null>;
};

type SingleEventObserver<T> = {
  event: Promise<Event<T>>;
};

export function observeEvent<T = any>(eventName: EventName): SingleEventObserver<T>;
export function observeEvent<T = any>(
  eventName: EventName,
  opts: Options<T>,
): SingleEventObserver<T>;
export function observeEvent<T = any>(
  eventName: EventName,
  opts: AbortableOptions<T>,
): SingleEventAbortableObserver<T>;

export function observeEvent<T = any>(
  eventName: EventName,
  {
    chain = 'chainflip',
    test = () => true,
    finalized = false,
    historicalCheckBlocks = 0,
    abortable = false,
  }: Options<T> | AbortableOptions<T> = {},
): SingleEventObserver<T> | SingleEventAbortableObserver<T> {
  if (abortable) {
    const observer = observeEvents(eventName, {
      chain,
      test,
      finalized,
      historicalCheckBlocks,
      abortable,
    });

    return {
      stop: () => observer.stop(),
      event: observer.events.then((events) => events?.[0]),
    } as SingleEventAbortableObserver<T>;
  }

  const observer = observeEvents(eventName, {
    chain,
    test,
    finalized,
    historicalCheckBlocks,
    abortable,
  });

  return {
    // Just return the first matching event
    event: observer.events.then((events) => events[0]),
  } as SingleEventObserver<T>;
}

/* eslint-disable @typescript-eslint/no-explicit-any */
export function observeBadEvent<T = any>(
  eventName: EventName,
  { test, label }: { test?: EventTest<T>; label?: string },
): { stop: () => Promise<void> } {
  const observer = observeEvent(eventName, { test, abortable: true });

  return {
    stop: async () => {
      observer.stop();

      await observer.event.then((event) => {
        if (event) {
          throw new Error(
            `Unexpected event emitted ${event.name.section}:${event.name.method} in block ${event.block} [${label}]`,
          );
        }
      });
    },
  };
}
