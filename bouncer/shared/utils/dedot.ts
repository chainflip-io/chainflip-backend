import type { DedotClient } from 'dedot';
import { InvalidTxError } from 'dedot';
import type { DispatchError } from 'dedot/codecs';
import type {
  IKeyringPair,
  IRuntimeTxCall,
  ISubmittableResult as DedotSubmittableResult,
} from 'dedot/types';
import type { ChainflipNodeApi } from 'generated/chaintypes/chainflip-node';
import type { ChainSubmittableExtrinsic } from 'generated/chaintypes/chainflip-node/tx';
import { bigintReplacer, cfMutex, sleep } from 'shared/utils';

/** A fully-typed dedot client for the Chainflip state chain. */
export type ChainflipClient = DedotClient<ChainflipNodeApi>;

/**
 * Common supertype for any `client.tx.<pallet>.<call>(...)` extrinsic. The per-call return type is
 * invariant in its metadata, so the base `IRuntimeTxCall` makes them all assignable here.
 */
export type ChainflipExtrinsic = ChainSubmittableExtrinsic<
  IRuntimeTxCall,
  ChainflipNodeApi['types']
>;

/** A human-readable `pallet.call(args)` description of a dedot extrinsic, for logging. */
export function extrinsicToHumanReadable(ext: ChainflipExtrinsic): string {
  const { pallet, palletCall } = ext.call;
  if (typeof palletCall === 'string') {
    return `${pallet}.${palletCall}()`;
  }
  if (!palletCall) {
    return `${pallet}()`;
  }
  const params = 'params' in palletCall ? palletCall.params : undefined;
  return `${pallet}.${palletCall.name}(${JSON.stringify(params ?? {}, bigintReplacer)})`;
}

/** The pallet + variant names (and docs) of a `Module` dispatch error; undefined for other kinds. */
export function moduleErrorMeta(
  client: ChainflipClient,
  err: DispatchError,
): { pallet: string; name: string; docs: string[] } | undefined {
  if (err.type === 'Module') {
    const meta = client.registry.findErrorMeta(err);
    if (meta) {
      // Lower-case the first char to match dedot's `client.errors` keys (the `DispatchErrorMatch`
      // pallet names) and the historical `pallet.Error` message format.
      const pallet = meta.pallet.charAt(0).toLowerCase() + meta.pallet.slice(1);
      return { pallet, name: meta.name, docs: meta.docs };
    }
  }
  return undefined;
}

/** Formats a dedot `DispatchError` as `pallet.Error: docs`. */
export function formatDispatchError(client: ChainflipClient, err: DispatchError): string {
  const meta = moduleErrorMeta(client, err);
  if (meta) {
    return `${meta.pallet}.${meta.name}: ${meta.docs.join(' ')}`;
  }
  return JSON.stringify(err, bigintReplacer);
}

/**
 * Thrown by {@link signSendAndWait} when an extrinsic's dispatch fails. For a `Module` error it
 * carries the pallet/variant names so callers can match structurally via {@link isDispatchError}
 * rather than substring-matching the message.
 */
export class ExtrinsicSubmissionError extends Error {
  declare readonly module?: { pallet: string; name: string };

  constructor(message: string, module?: { pallet: string; name: string }) {
    super(message);
    this.name = 'ExtrinsicSubmissionError';
    Object.defineProperty(this, 'module', { value: module, enumerable: false });
  }
}

/**
 * `client.errors` with dedot's bare-`string` index signatures dropped (cf. {@link StrictChainTx}),
 * so the pallet keys and per-pallet variant names are exact string literals rather than `string`.
 */
type StrictChainErrors = {
  [Pallet in keyof RemoveIndex<ChainflipNodeApi['errors']>]: RemoveIndex<
    ChainflipNodeApi['errors'][Pallet]
  >;
};
type ErrorPallet = keyof StrictChainErrors & string;
type ErrorNameOf<P extends ErrorPallet> = keyof StrictChainErrors[P] & string;
type AnyErrorName = { [P in ErrorPallet]: ErrorNameOf<P> }[ErrorPallet];

/**
 * A compile-time-checked dispatch-error selector. `name` must be a real error variant of `pallet`
 * (a typo or wrong pallet is a compile error). Omit `pallet` to match a variant regardless of which
 * (instanced) pallet emitted it — e.g. `{ name: 'BelowEgressDustLimit' }` matches any chain's
 * ingress-egress pallet.
 */
export type DispatchErrorMatch =
  | { [P in ErrorPallet]: { pallet: P; name: ErrorNameOf<P> } }[ErrorPallet]
  | { pallet?: undefined; name: AnyErrorName };

/** True if `err` is a dispatch failure matching `match` (see {@link DispatchErrorMatch}). */
export function isDispatchError(err: unknown, match: DispatchErrorMatch): boolean {
  return (
    err instanceof ExtrinsicSubmissionError &&
    err.module !== undefined &&
    (match.pallet === undefined || err.module.pallet === match.pallet) &&
    err.module.name === match.name
  );
}

/**
 * Per-account next-nonce cache. The node's `system_accountNextIndex` doesn't reflect pool-only txs,
 * so concurrent reads collide and the later tx is rejected as `Stale`. Allocating `max(on-chain,
 * cached)` and bumping the cache lets same-account submissions pipeline with sequential nonces.
 */
const nextNonceByAccount = new Map<string, number>();

/** Minimal async concurrency limiter: `limit(fn)` runs at most `max` `fn`s at once. */
function createConcurrencyLimiter(max: number) {
  let active = 0;
  const queue: (() => void)[] = [];
  const release = () => {
    const next = queue.shift();
    if (next) {
      next(); // transfer the slot to the next waiter; `active` is unchanged
    } else {
      active -= 1;
    }
  };
  return async function limit<T>(fn: () => Promise<T>): Promise<T> {
    if (active < max) {
      active += 1;
    } else {
      await new Promise<void>((resolve) => {
        queue.push(resolve);
      });
    }
    try {
      return await fn();
    } finally {
      release();
    }
  };
}

/**
 * Caps concurrent broadcasts process-wide. The node limits concurrent `transaction_v1_broadcast`
 * operations per connection to 16 (`MAX_TRANSACTION_PER_CONNECTION`), and we share one connection;
 * exceeding it throws "Maximum number of broadcasted transactions has been reached". Kept below 16
 * (slots free at inclusion, so this rarely blocks).
 */
const broadcastLimit = createConcurrencyLimiter(12);

/**
 * A submission result guaranteed to be in a block, so `status.value.blockNumber`/`txIndex` are
 * always present and callers can read them without re-checking the status.
 */
export type IncludedResult = DedotSubmittableResult & {
  status: Extract<DedotSubmittableResult['status'], { value: { blockNumber: number } }>;
};

/** Waits until the finalized block has caught up to the current best block (bounded by `timeoutSeconds`). */
async function waitForFinalizedToReachBest(
  client: ChainflipClient,
  timeoutSeconds: number,
): Promise<void> {
  const target = (await client.block.best()).number;
  for (let i = 0; i < timeoutSeconds; i += 1) {
    if ((await client.block.finalized()).number >= target) {
      return;
    }
    await sleep(1000);
  }
}

/**
 * Signs and submits `ext`, resolving once it is included in a best-chain block (the localnet best
 * chain is canonical and inclusion frees the broadcast slot sooner). Pass `waitForFinalize = true`
 * to instead wait for finalization. Throws if the tx is dropped/invalid, its dispatch failed, or
 * it isn't included within `timeoutSeconds` — so the returned result is always a successful,
 * included one (no need to re-check `dispatchError`). The nonce is allocated under `cfMutex` keyed
 * by `mutexKey`, then the lock is released so same-account submissions stay pipelined.
 */
export async function signSendAndWait(
  client: ChainflipClient,
  ext: ChainflipExtrinsic,
  signer: IKeyringPair,
  mutexKey: string,
  timeoutSeconds = 20,
  waitForFinalize = false,
): Promise<IncludedResult> {
  // Bound concurrent in-flight broadcasts to stay under the node's broadcast limit.
  return broadcastLimit(async () => {
    // dedot validates each tx against the FINALIZED block before broadcasting, which can lag a
    // state change the caller just observed via the indexer (best block) — e.g. a fresh funding,
    // or a new governance membership that gates a fee waiver. On such a (transient) pre-validation
    // failure, wait for finalization to catch up to the current best block and retry once.
    for (let attempt = 0; ; attempt += 1) {
      // Allocate a nonce under the mutex, then release immediately.
      const release = await cfMutex.acquire(mutexKey);
      let nonce: number;
      try {
        const onChain = Number(await client.rpc.system_accountNextIndex(signer.address));
        nonce = Math.max(onChain, nextNonceByAccount.get(signer.address) ?? 0);
        nextNonceByAccount.set(signer.address, nonce + 1);
      } finally {
        release();
      }

      // Whether the tx reached a block (nonce consumed on-chain). A dispatch failure still
      // consumed the nonce; only a never-included tx leaves the cache ahead and needs a reset.
      let included = false;
      let stopBroadcast: (() => Promise<void>) | undefined;
      try {
        const result = await new Promise<IncludedResult>((resolve, reject) => {
          const timer = setTimeout(() => {
            reject(
              new Error(
                `'${extrinsicToHumanReadable(ext)}' did not reach ${
                  waitForFinalize ? 'finalization' : 'a block'
                } within ${timeoutSeconds}s`,
              ),
            );
          }, timeoutSeconds * 1000);

          ext
            .signAndSend(signer, { nonce }, (res: DedotSubmittableResult) => {
              switch (res.status.type) {
                case 'BestChainBlockIncluded':
                  // The tx is in a block (nonce consumed) regardless of whether we keep waiting.
                  included = true;
                  if (!waitForFinalize) {
                    clearTimeout(timer);
                    resolve(res as IncludedResult);
                  }
                  break;
                case 'Finalized':
                  included = true;
                  clearTimeout(timer);
                  resolve(res as IncludedResult);
                  break;
                case 'Invalid':
                case 'Drop':
                  clearTimeout(timer);
                  reject(
                    new Error(
                      `Extrinsic failed with status ${res.status.type}: ${res.status.value.error}`,
                    ),
                  );
                  break;
                default:
                  break;
              }
            })
            .then((unsub) => {
              stopBroadcast = unsub;
            })
            .catch((err) => {
              clearTimeout(timer);
              reject(err);
            });
        });

        if (result.dispatchError) {
          const meta = moduleErrorMeta(client, result.dispatchError);
          throw new ExtrinsicSubmissionError(
            `'${extrinsicToHumanReadable(ext)}' failed (${formatDispatchError(client, result.dispatchError)})`,
            meta && { pallet: meta.pallet, name: meta.name },
          );
        }

        return result;
      } catch (e) {
        if (!included) {
          nextNonceByAccount.delete(signer.address);
        }
        // Rethrow unless this is a transient finalized-block pre-validation failure we can retry
        // once (see note at the top of the loop). Otherwise wait for finalization to catch up and
        // fall through to the next loop iteration.
        if (included || attempt > 0 || !(e instanceof InvalidTxError)) {
          throw e;
        }
        await waitForFinalizedToReachBest(client, timeoutSeconds);
      } finally {
        // Stop the broadcast + block tracking so the node frees the broadcast slot immediately
        // (dedot only auto-stops it at finalization).
        await stopBroadcast?.().catch(() => undefined);
      }
    }
  });
}

/**
 * dedot's generated `client.tx` carries bare-`string` index signatures, so misspelled pallet/call
 * names compile and only fail at runtime. The types below drop those index keys (keeping every
 * literal-named key and its arg types) so typos are compile errors. Type-level only — `strictTx`
 * is the identity at runtime and survives chaintypes regeneration.
 */

/** Drop bare `string`/`number` index signatures, keep literal-named keys. */
type RemoveIndex<T> = {
  [K in keyof T as string extends K ? never : number extends K ? never : K]: T[K];
};

/** A `client.tx` whose pallets and calls are exactly those in the metadata. */
export type StrictChainTx<Tx> = {
  [Pallet in keyof RemoveIndex<Tx>]: RemoveIndex<Tx[Pallet]>;
};

/** Identity at runtime; returns the strict (no-fallback) view of `client.tx`. */
export function strictTx(client: ChainflipClient): StrictChainTx<ChainflipNodeApi['tx']> {
  return client.tx as unknown as StrictChainTx<ChainflipNodeApi['tx']>;
}
