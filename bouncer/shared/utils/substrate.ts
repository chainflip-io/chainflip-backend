import 'disposablestack/auto';
import { deferredPromise, getChainflipApi, getPolkadotApi } from '../utils';

const apiMap = {
  chainflip: getChainflipApi,
  polkadot: getPolkadotApi,
} as const;

type SubstrateChain = keyof typeof apiMap;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type Event<T = any> = {
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
/* eslint-enable @typescript-eslint/no-explicit-any */

export function observeBadEvents<T>(
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
