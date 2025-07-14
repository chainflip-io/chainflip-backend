import 'disposablestack/auto';
import { ApiPromise, WsProvider } from '@polkadot/api';
import { Observable, Subject } from 'rxjs';
import { deferredPromise, runWithTimeout } from 'shared/utils';
import { globalLogger, Logger } from 'shared/utils/logger';
import { appendFileSync } from 'node:fs';
import { Header } from '@polkadot/types/interfaces';

// Set the STATE_CHAIN_EVENT_LOG_FILE env var to log all state chain events to a file. Used for debugging.
export const stateChainEventLogFile = process.env.STATE_CHAIN_EVENT_LOG_FILE; // ?? '/tmp/chainflip/state_chain_events.log';

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

export type DisposableApiPromise = ApiPromise & { [Symbol.asyncDispose](): Promise<void> };

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

export const CHAINFLIP_HTTP_ENDPOINT = process.env.CF_NODE_HTTP_ENDPOINT ?? 'http://127.0.0.1:9944';

export const getPolkadotApi = getCachedSubstrateApi(
  process.env.POLKADOT_ENDPOINT ?? 'ws://127.0.0.1:9947',
);

export const getAssethubApi = getCachedSubstrateApi(
  process.env.ASSETHUB_ENDPOINT ?? 'ws://127.0.0.1:9955',
);

const apiMap = {
  chainflip: getChainflipApi,
  polkadot: getPolkadotApi,
  assethub: getAssethubApi,
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
  private events: Map<string, Event[]>;

  private headers: Map<string, Header>;

  // Determines the cache size limit, beyond which we cull old blocks down to the cacheAgeLimit.
  private cacheSizeLimit: number;

  private cacheAgeLimit: number;

  private chain: SubstrateChain;

  private bestBlockNumber: number | undefined;

  public bestBlockHash: string | undefined;

  private finalisedBlockNumber: number | undefined;

  private outputFile: string | undefined;

  private logger: Logger | undefined;

  private newHeadsSubject: Subject<{ blockHash: string; events: Event[] }> | undefined;

  private finalisedHeadsSubject: Subject<{ blockHash: string; events: Event[] }> | undefined;

  private subscriptionDisposer: (() => Promise<void>) | undefined;

  constructor(cacheAgeLimit: number, chain: SubstrateChain, outputFile?: string, logger?: Logger) {
    this.events = new Map();
    this.headers = new Map();
    this.cacheAgeLimit = cacheAgeLimit;
    this.cacheSizeLimit = cacheAgeLimit * 2;
    this.chain = chain;
    this.finalisedBlockNumber = undefined;
    this.outputFile = outputFile;
    this.logger = logger;
    this.newHeadsSubject = undefined;
    this.finalisedHeadsSubject = undefined;
    this.subscriptionDisposer = undefined;
  }

  async eventsForHeader(blockHeader: Header, finalized: boolean = false): Promise<Event[]> {
    const api = await apiMap[this.chain]();

    const blockHash = blockHeader.hash.toString();
    const blockHeight = blockHeader.number.toNumber();

    // Update finalization info based on the stream type
    if (finalized) {
      this.finalisedBlockNumber = blockHeight;
    }

    // Not necessarily 100% correct: ideally we would trace the block headers back to the latest finalised block.
    // Should be good enough for most use cases.
    if (this.bestBlockNumber === undefined || blockHeight > this.bestBlockNumber) {
      this.bestBlockNumber = blockHeight;
      this.bestBlockHash = blockHash;
    }

    this.logger?.debug(
      `${finalized ? 'Finalized' : 'New'} block ${blockHeight} (${blockHash}) on chain ${this.chain} with finalised block number ${this.finalisedBlockNumber}`,
    );

    // Update the caches.
    if (!this.events.has(blockHash)) {
      this.logger?.debug('Updating event cache');
      this.headers.set(blockHash, blockHeader);

      const historicalApi = await api.at(blockHash);
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const rawEvents = (await historicalApi.query.system.events()) as unknown as any[];
      const events = mapEvents(rawEvents, blockHeight);

      // Log the events
      if (this.outputFile) {
        appendFileSync(
          this.outputFile,
          `Block ${blockHeight}:${blockHash}: ` + JSON.stringify(events) + '\n',
        );
      }

      // Update the cache.
      this.events.set(blockHash, events); // Remove old blocks to maintain cache size

      // If the cache size exceeds the size limit, remove old blocks up to the cacheAgeLimit.
      if (this.headers.size > this.cacheSizeLimit) {
        this.logger?.debug('Reducing cache');
        const oldHashes = Array.from(this.headers.entries()).filter(([_hash, header]) => {
          if (header.number.toNumber() < blockHeight - this.cacheAgeLimit) {
            return true;
          }
          return false;
        });
        oldHashes.forEach(([hash, header]) => {
          this.logger?.debug(`Removing old block ${header.number}:${hash} from cache`);
          this.headers.delete(hash);
          this.events.delete(hash);
        });
      }

      this.logger?.debug(
        `Cached ${events.length} events for block ${blockHeight} (${blockHash}) on chain ${this.chain}`,
      );

      return events;
    }
    this.logger?.debug(`Using cached events for block ${blockHeight} (${blockHash})`);
    return this.events.get(blockHash)!;
  }

  async getHistoricalEvents(startHash: string, historicalCheckBlocks: number): Promise<Event[]> {
    if (historicalCheckBlocks <= 0) {
      return [];
    }
    if (historicalCheckBlocks > this.cacheAgeLimit) {
      this.logger?.warn(
        `Historical check blocks (${historicalCheckBlocks}) exceeds cache size (${this.cacheAgeLimit}). Using cache size instead.`,
      );
    }
    const depthLimit = Math.min(historicalCheckBlocks, this.cacheAgeLimit);

    const api = await apiMap[this.chain]();
    const events: Event[][] = [];

    this.logger?.debug(
      `Checking historical events for chain ${this.chain} over the last ${depthLimit} blocks`,
    );

    let depth = 0;
    let currentHash = startHash;
    while (depth < depthLimit) {
      depth++;
      const currentHeader = (await api.rpc.chain.getHeader(currentHash)) as Header;
      const currentEvents = await this.eventsForHeader(currentHeader, false);

      this.logger?.debug(
        `Found ${currentEvents.length} events at depth ${depth} for block ${currentHeader.number.toNumber()} (${currentHeader.hash.toString()})`,
      );

      events.push(currentEvents);

      if (currentHeader.number.toNumber() === 0) {
        this.logger?.debug('Reached genesis block, stopping historical event retrieval');
        break;
      }

      currentHash = currentHeader.parentHash.toString();
    }

    return events.reverse().flat();
  }

  private async startBackgroundSubscription(): Promise<void> {
    if (this.subscriptionDisposer) {
      return; // Already running
    }

    const stack = new AsyncDisposableStack();
    const api = stack.use(await apiMap[this.chain]());

    this.newHeadsSubject = new Subject<{ blockHash: string; events: Event[] }>();
    this.finalisedHeadsSubject = new Subject<{ blockHash: string; events: Event[] }>();

    // Subscribe to all heads
    const unsubscribeAllHeads = await api.rpc.chain.subscribeAllHeads(async (header: Header) => {
      try {
        const events = await this.eventsForHeader(header, false);
        this.newHeadsSubject?.next({ blockHash: header.hash.toString(), events });
      } catch (error) {
        this.logger?.error('Error processing new head:', error);
        this.newHeadsSubject?.error(error);
      }
    });

    // Subscribe to finalised heads
    const unsubscribeFinalizedHeads = await api.rpc.chain.subscribeFinalizedHeads(
      async (header: Header) => {
        try {
          const events = await this.eventsForHeader(header, true);
          this.finalisedHeadsSubject?.next({ blockHash: header.hash.toString(), events });
        } catch (error) {
          this.logger?.error('Error processing finalised head:', error);
          this.finalisedHeadsSubject?.error(error);
        }
      },
    );

    stack.defer(unsubscribeAllHeads);
    stack.defer(unsubscribeFinalizedHeads);
    stack.defer(() => {
      this.newHeadsSubject?.complete();
      this.finalisedHeadsSubject?.complete();
      this.newHeadsSubject = undefined;
      this.finalisedHeadsSubject = undefined;
    });

    this.subscriptionDisposer = () => {
      this.subscriptionDisposer = undefined;
      return stack.disposeAsync();
    };
  }

  async getObservable(
    finalized: boolean = false,
  ): Promise<Observable<{ blockHash: string; events: Event[] }>> {
    await this.startBackgroundSubscription();

    if (finalized) {
      if (!this.finalisedHeadsSubject) {
        throw new Error('Finalised heads subscription not initialized');
      }
      return this.finalisedHeadsSubject.asObservable();
    }
    if (!this.newHeadsSubject) {
      throw new Error('New heads subscription not initialized');
    }
    return this.newHeadsSubject.asObservable();
  }

  async dispose(): Promise<void> {
    if (this.subscriptionDisposer) {
      await this.subscriptionDisposer();
    }
  }
}

const chainflipEventCache = new EventCache(100, 'chainflip', stateChainEventLogFile, globalLogger);
const polkadotEventCache = new EventCache(100, 'polkadot');
const assethubEventCache = new EventCache(100, 'assethub');
const eventCacheMap = {
  chainflip: chainflipEventCache,
  polkadot: polkadotEventCache,
  assethub: assethubEventCache,
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

const subscribeHeads = getCachedDisposable(
  async ({ chain, finalized = false }: { chain: SubstrateChain; finalized?: boolean }) => {
    const cache = eventCacheMap[chain];
    const observable = await cache.getObservable(finalized);

    return {
      observable,
      [Symbol.asyncDispose]() {
        return Promise.resolve();
      },
    };
  },
);

async function getPastEvents(
  chain: SubstrateChain,
  bestBlockHash: string,
  historicalCheckBlocks: number,
): Promise<Event[]> {
  if (historicalCheckBlocks <= 0) {
    return [];
  }
  return eventCacheMap[chain].getHistoricalEvents(bestBlockHash, historicalCheckBlocks);
}

type EventTest<T> = (event: Event<T>) => boolean;

interface BaseOptions<T> {
  chain?: SubstrateChain;
  test?: EventTest<T>;
  finalized?: boolean;
  historicalCheckBlocks?: number;
  timeoutSeconds?: number;
  stopAfter?: number | ((event: Event<T>) => boolean);
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
export function observeEvents<T = any>(logger: Logger, eventName: EventName): Observer<T>;
export function observeEvents<T = any>(
  logger: Logger,
  eventName: EventName,
  opts: Options<T>,
): Observer<T>;
export function observeEvents<T = any>(
  logger: Logger,
  eventName: EventName,
  opts: AbortableOptions<T>,
): AbortableObserver<T>;
export function observeEvents<T = any>(
  logger: Logger,
  eventName: EventName,
  {
    chain = 'chainflip',
    test = () => true,
    finalized = false,
    historicalCheckBlocks = 1,
    timeoutSeconds = 0,
    abortable = false,
    stopAfter = test,
  }: Options<T> | AbortableOptions<T> = {},
) {
  const [expectedSection, expectedMethod] = eventName.split(':');
  logger.trace(`Observing event ${eventName}`);

  const controller = abortable ? new AbortController() : undefined;

  const findEvent = async () => {
    const foundEvents: Event[] = [];
    await using subscription = await subscribeHeads({ chain, finalized });

    const subscriptionIterator = observableToIterable<{ blockHash: string; events: Event[] }>(
      subscription.observable,
      controller?.signal,
    );

    const checkEvents = (events: Event[], log: string) => {
      if (events.length === 0) {
        return false;
      }
      logger.debug(`Checking ${events.length} ${log} events for ${eventName}`);
      let stop = false;
      for (const event of events) {
        logger.trace(
          `Checking event ${event.name.section}:${event.name.method} from block ${event.block}`,
        );
        if (
          event.name.section.includes(expectedSection) &&
          event.name.method.includes(expectedMethod)
        ) {
          if (test(event)) {
            logger.debug(
              `Found matching event ${event.name.section}:${event.name.method} in block ${event.block}`,
            );
            foundEvents.push(event);
          }
          stop =
            stop || typeof stopAfter === 'function'
              ? (stopAfter as (event: Event) => boolean)(event)
              : foundEvents.length >= stopAfter;
          if (stop) {
            typeof stopAfter === 'function'
              ? logger.debug(
                  `Stopping after matching event: ${event.name.section}:${event.name.method}`,
                )
              : logger.debug(`Stopping after finding ${stopAfter} events`);
            break;
          }
        }
      }
      return stop;
    };

    // Wait for the subscription to emit the first batch of events, which
    // will update the best block number in the event cache: required for
    // gap-free historical query.
    const firstResult = await subscriptionIterator.next();
    if (!firstResult.value) {
      if (controller?.signal?.aborted) {
        logger.debug('Abort signal received before any events were emitted.');
      } else {
        logger.warn(
          'Subscription completed before any events were emitted. This may indicate a problem with the subscription.',
        );
      }
      return [];
    }
    const { blockHash, events } = firstResult.value;

    if (checkEvents(events, 'current')) {
      logger.debug(`Found ${foundEvents.length} ${eventName} events in the first batch.`);
      return foundEvents;
    }

    const historicalEvents = await getPastEvents(chain, blockHash, historicalCheckBlocks);

    // Check historical events first
    if (!checkEvents(historicalEvents, 'historical')) {
      logger.debug(`No ${eventName} events found in historical query.`);
      for await (const { events } of subscriptionIterator) {
        if (checkEvents(events, 'subscription')) {
          break;
        } else {
          logger.debug(`No ${eventName} events found in subscription.`);
        }
      }
    }
    logger.debug(`Found ${foundEvents.length} ${eventName} events.`);

    return foundEvents;
  };

  let events: Promise<Event<T>[]>;
  if (timeoutSeconds > 0) {
    events = runWithTimeout(
      findEvent(),
      timeoutSeconds,
      logger,
      `Timeout while waiting for event ${eventName}`,
    );
  } else {
    events = findEvent();
  }

  if (!abortable) {
    // If not abortable, just return the events
    return { events } as Observer<T>;
  }
  return {
    stop: () => controller!.abort(),
    events,
  } as AbortableObserver<T>;
}

type SingleEventAbortableObserver<T> = {
  stop: () => void;
  event: Promise<Event<T> | null>;
};

type SingleEventObserver<T> = {
  event: Promise<Event<T>>;
};

export function observeEvent<T = any>(logger: Logger, eventName: EventName): SingleEventObserver<T>;
export function observeEvent<T = any>(
  logger: Logger,
  eventName: EventName,
  opts: Options<T>,
): SingleEventObserver<T>;
export function observeEvent<T = any>(
  logger: Logger,
  eventName: EventName,
  opts: AbortableOptions<T>,
): SingleEventAbortableObserver<T>;

export function observeEvent<T = any>(
  logger: Logger,
  eventName: EventName,
  {
    chain = 'chainflip',
    test = () => true,
    finalized = false,
    historicalCheckBlocks,
    timeoutSeconds = 0,
    abortable = false,
  }: Options<T> | AbortableOptions<T> = {},
): SingleEventObserver<T> | SingleEventAbortableObserver<T> {
  if (abortable) {
    const observer = observeEvents(logger, eventName, {
      chain,
      test,
      finalized,
      historicalCheckBlocks,
      timeoutSeconds,
      abortable,
    });

    return {
      stop: () => observer.stop(),
      event: observer.events.then((events) => events?.[0]),
    } as SingleEventAbortableObserver<T>;
  }

  const observer = observeEvents(logger, eventName, {
    chain,
    test,
    finalized,
    historicalCheckBlocks,
    timeoutSeconds,
    abortable,
  });

  return {
    // Just return the first matching event
    event: observer.events.then((events) => events[0]),
  } as SingleEventObserver<T>;
}

/* eslint-disable @typescript-eslint/no-explicit-any */
export function observeBadEvent<T = any>(
  logger: Logger,
  eventName: EventName,
  { test }: { test?: EventTest<T> },
): { stop: () => Promise<void> } {
  const observer = observeEvent(logger, eventName, { test, abortable: true });

  return {
    stop: async () => {
      observer.stop();

      await observer.event.then((event) => {
        if (event) {
          throw new Error(
            `Unexpected event emitted ${event.name.section}:${event.name.method} in block ${event.block}`,
          );
        }
      });
    },
  };
}
