import 'disposablestack/auto';
import { ApiPromise, WsProvider } from '@polkadot/api';
import { Observable, Subject } from 'rxjs';
import { runWithTimeout } from 'shared/utils';
import { globalLogger, Logger } from 'shared/utils/logger';
import { AsyncQueue } from 'shared/utils/async_queue';
import { appendFileSync } from 'node:fs';
import { EventRecord, Header } from '@polkadot/types/interfaces';

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
        EthEncodingType: {
          _enum: ['Domain', 'Eip712'],
        },
        SolEncodingType: {
          _enum: ['Domain'],
        },
        UserSignatureData: {
          _enum: {
            Solana: '(SolSignature, SolAddress, SolEncodingType)',
            Ethereum: '(EthereumSignature, EthereumAddress, EthEncodingType)',
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

  private finalisedBlockHash: string | undefined;

  private outputFile: string | undefined;

  private logger: Logger | undefined;

  private newHeadsSubject: Subject<{ header: Header; events: Event[] }> | undefined;

  private finalisedHeadsSubject: Subject<{ header: Header; events: Event[] }> | undefined;

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
    const blockHash = blockHeader.hash.toString();
    const blockHeight = blockHeader.number.toNumber();

    // Update finalization info based on the stream type
    if (finalized) {
      this.finalisedBlockNumber = blockHeight;
      this.finalisedBlockHash = blockHash;
    }

    // Not necessarily 100% correct: ideally we would trace the block headers back to the latest finalised block.
    // Should be good enough for most use cases.
    if (this.bestBlockNumber === undefined || blockHeight > this.bestBlockNumber) {
      this.bestBlockNumber = blockHeight;
      this.bestBlockHash = blockHash;
    }

    // Update the caches.
    if (!this.events.has(blockHash)) {
      this.logger?.trace(
        `Caching new ${finalized ? 'finalized' : ''} block ${blockHeight} (${blockHash}) on chain ${this.chain} with finalised block number ${this.finalisedBlockNumber}`,
      );

      this.headers.set(blockHash, blockHeader);

      const api = await (await apiMap[this.chain]()).at(blockHash);
      const rawEvents = (await api.query.system.events()) as unknown as EventRecord[];
      const events = rawEvents.map(({ event }) => ({
        name: { section: event.section, method: event.method },
        data: event.data.toHuman(),
        block: blockHeader.number.toNumber(),
        eventIndex: event.index as unknown as number,
      }));

      // Log the events
      if (this.outputFile) {
        appendFileSync(
          this.outputFile,
          `Block ${blockHeight}:${blockHash}: ` + JSON.stringify(events) + '\n',
        );
      }

      // Update the cache.
      this.events.set(blockHash, events);

      // If the cache size exceeds the size limit, remove old blocks up to the cacheAgeLimit.
      if (this.headers.size > this.cacheSizeLimit) {
        this.logger?.trace('Reducing cache');
        const oldHashes = Array.from(this.headers.entries()).filter(([_hash, header]) => {
          if (header.number.toNumber() < blockHeight - this.cacheAgeLimit) {
            return true;
          }
          return false;
        });
        oldHashes.forEach(([hash, header]) => {
          this.logger?.trace(`Removing old block ${header.number}:${hash} from cache`);
          this.headers.delete(hash);
          this.events.delete(hash);
        });
      }

      this.logger?.trace(
        `Cached ${events.length} events for block ${blockHeight} (${blockHash}) on chain ${this.chain}`,
      );

      return events;
    }
    this.logger?.trace(`Using cached events for block ${blockHeight} (${blockHash})`);
    return this.events.get(blockHash)!;
  }

  async getHistoricalEvents(bestHeader: Header, historicalCheckBlocks: number): Promise<Event[]> {
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

    this.logger?.trace(
      `Checking historical events for chain ${this.chain} over the last ${depthLimit} blocks`,
    );

    let depth = 0;
    let currentHash = bestHeader.parentHash.toString();
    while (depth < depthLimit) {
      depth++;
      const currentHeader = (await api.rpc.chain.getHeader(currentHash)) as Header;
      const currentEvents = await this.eventsForHeader(currentHeader, false);

      this.logger?.trace(
        `Found ${currentEvents.length} events at depth ${depth} for block ${currentHeader.number.toNumber()} (${currentHeader.hash.toString()})`,
      );

      events.push(currentEvents);

      if (currentHeader.number.toNumber() === 0) {
        this.logger?.trace('Reached genesis block, stopping historical event retrieval');
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

    this.newHeadsSubject = new Subject<{ header: Header; events: Event[] }>();
    this.finalisedHeadsSubject = new Subject<{ header: Header; events: Event[] }>();

    // Subscribe to all heads
    const unsubscribeAllHeads = await api.rpc.chain.subscribeAllHeads(async (header: Header) => {
      try {
        const events = await this.eventsForHeader(header, false);
        this.newHeadsSubject?.next({ header, events });
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
          this.finalisedHeadsSubject?.next({ header, events });
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
  ): Promise<Observable<{ header: Header; events: Event[] }>> {
    await this.startBackgroundSubscription();

    if (finalized) {
      if (!this.finalisedHeadsSubject) {
        throw new Error('Finalised heads subscription not initialized');
      }
      return new Observable((subscriber) => {
        if (
          this.finalisedBlockHash &&
          this.headers.has(this.finalisedBlockHash) &&
          this.events.has(this.finalisedBlockHash)
        ) {
          subscriber.next({
            header: this.headers.get(this.finalisedBlockHash)!,
            events: this.events.get(this.finalisedBlockHash)!,
          });
        }
        const subscription = this.finalisedHeadsSubject!.subscribe({
          next: (value) => subscriber.next(value),
          error: (error) => subscriber.error(error),
          complete: () => subscriber.complete(),
        });

        return () => subscription.unsubscribe();
      });
    }
    if (!this.newHeadsSubject) {
      throw new Error('New heads subscription not initialized');
    }
    return new Observable((subscriber) => {
      if (
        this.bestBlockHash &&
        this.headers.has(this.bestBlockHash) &&
        this.events.has(this.bestBlockHash)
      ) {
        subscriber.next({
          header: this.headers.get(this.bestBlockHash)!,
          events: this.events.get(this.bestBlockHash)!,
        });
      }
      const subscription = this.newHeadsSubject!.subscribe({
        next: (value) => subscriber.next(value),
        error: (error) => subscriber.error(error),
        complete: () => subscriber.complete(),
      });

      return () => subscription.unsubscribe();
    });
  }

  async dispose(): Promise<void> {
    if (this.subscriptionDisposer) {
      await this.subscriptionDisposer();
    }
  }
}

const chainflipEventCache = new EventCache(100, 'chainflip', stateChainEventLogFile, globalLogger);
const polkadotEventCache = new EventCache(100, 'polkadot', undefined, globalLogger);
const assethubEventCache = new EventCache(100, 'assethub', undefined, globalLogger);
const eventCacheMap = {
  chainflip: chainflipEventCache,
  polkadot: polkadotEventCache,
  assethub: assethubEventCache,
} as const;

async function* observableToIterable<T>(observer: Observable<T>, signal?: AbortSignal) {
  const queue = new AsyncQueue<T>();

  if (signal) {
    signal.addEventListener('abort', () => queue.end(), { once: true });
  }

  const sub = observer.subscribe({
    next: (value: T) => queue.push(value),
    error: () => {
      queue.end();
    },
    complete: () => queue.end(),
  });

  try {
    for await (const value of queue) {
      yield value;
    }
  } finally {
    sub.unsubscribe();
  }
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
  bestHeader: Header,
  historicalCheckBlocks: number,
): Promise<Event[]> {
  if (historicalCheckBlocks <= 0) {
    return [];
  }
  return eventCacheMap[chain].getHistoricalEvents(bestHeader, historicalCheckBlocks);
}

type EventTest<T> = (event: Event<T>) => boolean;

interface BaseOptions<T> {
  chain?: SubstrateChain;
  test?: EventTest<T>;
  finalized?: boolean;
  historicalCheckBlocks?: number;
  timeoutSeconds?: number;
  stopAfter?: { blocks: number } | 'Never' | 'Any';
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
    historicalCheckBlocks = 0,
    timeoutSeconds = 0,
    abortable = false,
    stopAfter = 'Any',
  }: Options<T> | AbortableOptions<T> = {},
) {
  const [expectedSection, expectedMethod] = eventName.split(':');
  const startTime = Date.now();
  logger.debug(`Observing event ${eventName}`);

  const controller = abortable ? new AbortController() : undefined;

  const logMessagePeriod: number = 5;
  let blocksChecked = 0;
  const findEvent = async () => {
    await using subscription = await subscribeHeads({ chain, finalized });

    const subscriptionIterator = observableToIterable<{ header: Header; events: Event[] }>(
      subscription.observable,
      controller?.signal,
    );

    const checkEvents = (events: Event[], log: string) => {
      blocksChecked += 1;
      if (blocksChecked % logMessagePeriod === 0) {
        logger.trace(
          `Checking ${events.length} ${log} events for ${eventName} (${blocksChecked}th block)`,
        );
      }
      let stop = false;
      const foundEvents: Event[] = [];
      for (const event of events) {
        if (
          event.name.section.includes(expectedSection) &&
          event.name.method.includes(expectedMethod)
        ) {
          if (test(event)) {
            logger.debug(
              `Found matching event ${event.name.section}:${event.name.method} in block ${event.block}`,
            );
            foundEvents.push(event);
            if (stopAfter === 'Any') {
              stop = true;
            }
          }
        }
      }
      if (typeof stopAfter === 'object' && 'blocks' in stopAfter) {
        stop = stop || blocksChecked >= stopAfter.blocks;
      }
      return { stop, foundEvents };
    };

    const latestResult = await subscriptionIterator.next();
    if (!latestResult.value) {
      if (controller?.signal?.aborted) {
        logger.debug('Abort signal received before any events were emitted.');
      } else {
        logger.warn(
          'Subscription completed before any events were emitted. This may indicate a problem with the subscription.',
        );
      }
      return [];
    }

    // eslint-disable-next-line prefer-const
    let { stop, foundEvents } = checkEvents(latestResult.value.events, 'current');
    if (stop) {
      logger.debug(`Found ${foundEvents.length} ${eventName} events in the first batch.`);
      return foundEvents;
    }
    logger.trace(`No ${eventName} events found in the first batch.`);

    if (historicalCheckBlocks > 0) {
      // eslint-disable-next-line @typescript-eslint/no-shadow
      const { stop, foundEvents: historicalEvents } = checkEvents(
        await getPastEvents(chain, latestResult.value.header, historicalCheckBlocks),
        'historical',
      );
      foundEvents = [...historicalEvents, ...foundEvents];
      if (stop) {
        logger.debug(`Found ${historicalEvents.length} ${eventName} events in historical query.`);
        return foundEvents;
      }
      logger.trace(`No historical ${eventName} events found.`);
    }

    for await (const { events } of subscriptionIterator) {
      // eslint-disable-next-line @typescript-eslint/no-shadow
      const { stop, foundEvents: nextEvents } = checkEvents(events, 'subscription');
      foundEvents = [...foundEvents, ...nextEvents];
      if (stop) {
        logger.debug(
          `Found ${foundEvents.length} ${eventName} events, took ${Math.round(
            (Date.now() - startTime) / 1000,
          )} seconds`,
        );
        break;
      } else if (blocksChecked % logMessagePeriod === 0) {
        logger.trace(`No ${eventName} events found in subscription.`);
      }
    }

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
