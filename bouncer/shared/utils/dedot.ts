import type { DedotClient } from 'dedot';
import type { DispatchError } from 'dedot/codecs';
import type {
  IKeyringPair,
  IRuntimeTxCall,
  ISubmittableResult as DedotSubmittableResult,
} from 'dedot/types';
import type {
  ChainflipNodeApi,
  CfChainsAddressEncodedAddress,
} from 'generated/chaintypes/chainflip-node';
import type { ChainSubmittableExtrinsic } from 'generated/chaintypes/chainflip-node/tx';
import { cfMutex, shortChainFromChain, type Chain } from 'shared/utils';

/** A fully-typed dedot client for the Chainflip state chain. */
export type ChainflipClient = DedotClient<ChainflipNodeApi>;

/**
 * Builds a typed `EncodedAddress` from a chain and a pre-encoded address value.
 * `address` should already be in the chain's on-chain encoding (hex for EVM/Sol/Dot/Hub,
 *  hex-encoded bytes for Btc).
 */
export function encodedAddress(chain: Chain, address: string): CfChainsAddressEncodedAddress {
  return { type: shortChainFromChain(chain), value: address } as CfChainsAddressEncodedAddress;
}

/**
 * A signed-or-unsigned extrinsic built from `client.tx.<pallet>.<call>(...)`.
 *
 * The generated `client.tx.*.*()` calls each return `ChainSubmittableExtrinsic<<call-meta>,
 * ChainKnownTypes>`, invariant in the per-call metadata type. Using the base `IRuntimeTxCall`
 * constraint here makes this the common supertype every call return is assignable to, while
 * argument/call/pallet checking still happens inside the closure against the typed client.
 */
export type ChainflipExtrinsic = ChainSubmittableExtrinsic<
  IRuntimeTxCall,
  ChainflipNodeApi['types']
>;

const bigintReplacer = (_key: string, value: unknown) =>
  typeof value === 'bigint' ? value.toString() : value;

/**
 * A human-readable `pallet.call(args)` description of a dedot extrinsic, for logging.
 * Mirrors the `section.method(args)` strings the polkadot.js path produced via `toHuman()`.
 */
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

/**
 * Formats a dedot `DispatchError` into the same `pallet.Error: docs` string.
 * e.g. `lendingPools.AccountNotFoundInPool`.
 */
export function formatDispatchError(client: ChainflipClient, err: DispatchError): string {
  if (err.type === 'Module') {
    const meta = client.registry.findErrorMeta(err);
    if (meta) {
      const pallet = meta.pallet.charAt(0).toLowerCase() + meta.pallet.slice(1);
      return `${pallet}.${meta.name}: ${meta.docs.join(' ')}`;
    }
  }
  return JSON.stringify(err, bigintReplacer);
}

/**
 * Signs `ext` with `signer`, submits it, and resolves once it is in a finalized block.
 *
 * Holds the `cfMutex` keyed by `mutexKey` until the tx is in the pool, so a subsequent
 * submission for the same key reads the correct (incremented) nonce; the mutex is released
 * early in the status callback and again on any exit.
 *
 * Throws if the extrinsic was dropped/invalid or if its dispatch failed, so the returned
 * result is always a successful, finalized one — the caller can use `result.status.value`
 * and `result.events` without re-checking `dispatchError`.
 */
export async function signSendAndFinalize(
  client: ChainflipClient,
  ext: ChainflipExtrinsic,
  signer: IKeyringPair,
  mutexKey: string,
): Promise<DedotSubmittableResult> {
  const release = await cfMutex.acquire(mutexKey);
  let released = false;
  const releaseOnce = () => {
    if (!released) {
      released = true;
      release();
    }
  };

  try {
    const nonce = await client.rpc.system_accountNextIndex(signer.address);
    const result = await new Promise<DedotSubmittableResult>((resolve, reject) => {
      ext
        .signAndSend(signer, { nonce }, (res: DedotSubmittableResult) => {
          switch (res.status.type) {
            case 'Broadcasting':
            case 'BestChainBlockIncluded':
              releaseOnce();
              break;
            case 'Finalized':
              releaseOnce();
              resolve(res);
              break;
            case 'Invalid':
            case 'Drop':
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
        .catch(reject);
    });

    if (result.dispatchError) {
      throw new Error(
        `'${extrinsicToHumanReadable(ext)}' failed (${formatDispatchError(client, result.dispatchError)})`,
      );
    }

    return result;
  } finally {
    releaseOnce();
  }
}

/**
 * Closing dedot's "fallback" gap so misspelled pallet/call names fail at compile time.
 *
 * dedot's generated chain types carry two bare-`string` index signatures (from
 * `@dedot/codecs` `GenericChainTx`):
 *   - top level:   `[pallet: string]:   { ... }`           -> unknown pallets compile
 *   - per pallet:  `[callName: string]: GenericTxCall<...>` -> unknown calls compile
 *
 * There is no codegen flag to turn these off. But a mapped type with an `as` clause can
 * drop the bare `string` / `number` index keys while preserving every literal-named key
 * (and its precise argument types). Applying it at both levels yields a `client.tx` view
 * where a typo is a compile error instead of a runtime `Tx call spec not found`.
 *
 * This is purely type-level: `strictTx` is the identity function at runtime, so it has
 * zero cost and survives `dedot chaintypes` regeneration (we never edit generated files).
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
