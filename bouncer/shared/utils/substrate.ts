import 'disposablestack/auto';
import { ApiPromise, WsProvider } from '@polkadot/api';
import { deferredPromise } from '../utils';

// @ts-expect-error polyfilling
Symbol.asyncDispose ??= Symbol('asyncDispose');
// @ts-expect-error polyfilling
Symbol.dispose ??= Symbol('dispose');

type DisposableApiPromise = ApiPromise & { [Symbol.asyncDispose](): Promise<void> };

// It is important to cache WS connections because nodes seem to have a
// limit on how many can be opened at the same time (from the same IP presumably)
function getCachedSubstrateApi(defaultEndpoint: string) {
  let api: DisposableApiPromise | undefined;
  let connections = 0;

  return async (providedEndpoint?: string): Promise<DisposableApiPromise> => {
    if (!api) {
      const endpoint = providedEndpoint ?? defaultEndpoint;

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

      api = new Proxy(apiPromise as unknown as DisposableApiPromise, {
        get(target, prop, receiver) {
          if (prop === Symbol.asyncDispose) {
            return async () => {
              connections -= 1;
              if (connections === 0) {
                setTimeout(() => {
                  if (connections === 0) {
                    api = undefined;
                    Reflect.get(target, 'disconnect', receiver)
                      .call(target)
                      .catch(() => null);
                  }
                }, 5_000).unref();
              }
            };
          }
          if (prop === 'disconnect') {
            return async () => {
              // noop
            };
          }

          return Reflect.get(target, prop, receiver);
        },
      });
    }

    connections += 1;

    return api;
  };
}

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

async function* subscribeHeads({
  chain,
  finalized = false,
  signal,
}: {
  chain: SubstrateChain;
  finalized?: boolean;
  signal?: AbortSignal;
}) {
  // take the correct substrate API
  await using api = await apiMap[chain]();
  // prepare a stack for cleanup
  using stack = new DisposableStack();

  // subscribe to the correct head based on the finalized flag
  const subscribe = finalized
    ? api.rpc.chain.subscribeFinalizedHeads
    : api.rpc.chain.subscribeNewHeads;

  // async generator is pull-based, but the subscribe new heads is push-based
  // if the consumer takes too long, we need to buffer the events
  const buffer: Event[][] = [];

  // yield the first batch of events via a promise because it is asynchronous
  let promise: Promise<Event[]> | undefined;
  let resolve: ((value: Event[]) => void) | undefined;
  let reject: ((error: Error) => void) | undefined;

  signal?.addEventListener('abort', () => {
    reject?.(new Error('Aborted'));
  });

  ({ resolve, promise, reject } = deferredPromise<Event[]>());

  const unsubscribe = await subscribe(async (header) => {
    const historicApi = await api.at(header.hash);

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const rawEvents = (await historicApi.query.system.events()) as unknown as any[];
    const events: Event[] = [];

    // iterate over all the events, reshaping them for the consumer
    rawEvents.forEach(({ event }, index) => {
      events.push({
        name: { section: event.section, method: event.method },
        data: event.toHuman().data,
        block: header.number.toNumber(),
        eventIndex: index,
      });
    });

    // if we haven't consumed the promise yet, resolve it and prepare the for
    // the next batch, otherwise begin buffering the events
    if (resolve) {
      resolve(events);
      promise = undefined;
      resolve = undefined;
      reject = undefined;
    } else {
      buffer.push(events);
    }
  });

  // automatic cleanup!
  stack.defer(unsubscribe);

  while (true) {
    const next = await promise.catch(() => null);

    // yield the first batch
    if (next === null) break;
    yield* next;

    // if the consume took too long, yield the buffered events
    while (buffer.length !== 0) {
      yield* buffer.shift()!;
    }

    // reset for the next batch
    ({ resolve, promise, reject } = deferredPromise<Event[]>());
  }
}

type EventTest<T> = (event: Event<T>) => boolean;

interface BaseOptions<T> {
  chain?: SubstrateChain;
  test?: EventTest<T>;
  finalized?: boolean;
}

interface Options<T> extends BaseOptions<T> {
  abortable?: false;
}

interface AbortableOptions<T> extends BaseOptions<T> {
  abortable: true;
}

type EventName = `${string}:${string}`;

type Observer<T> = {
  event: Promise<Event<T>>;
};

type AbortableObserver<T> = {
  stop: () => void;
  event: Promise<Event<T> | null>;
};

/* eslint-disable @typescript-eslint/no-explicit-any */
export function observeEvent<T = any>(eventName: EventName): Observer<T>;
export function observeEvent<T = any>(eventName: EventName, opts: Options<T>): Observer<T>;
export function observeEvent<T = any>(
  eventName: EventName,
  opts: AbortableOptions<T>,
): AbortableObserver<T>;
export function observeEvent<T = any>(
  eventName: EventName,
  {
    chain = 'chainflip',
    test = () => true,
    finalized = false,
    abortable = false,
  }: Options<T> | AbortableOptions<T> = {},
) {
  const [expectedSection, expectedMethod] = eventName.split(':');

  const controller = abortable ? new AbortController() : undefined;

  const it = subscribeHeads({ chain, finalized });

  controller?.signal.addEventListener('abort', () => {
    /* eslint-disable-next-line @typescript-eslint/no-floating-promises */
    it.return();
  });

  const findEvent = async () => {
    for await (const event of it) {
      if (
        event.name.section.includes(expectedSection) &&
        event.name.method.includes(expectedMethod) &&
        test(event)
      ) {
        return event as Event<T>;
      }
    }

    return null;
  };

  if (!controller) return { event: findEvent() } as Observer<T>;

  return { stop: () => controller.abort(), event: findEvent() } as AbortableObserver<T>;
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
