import {
  assethubSigningMutex,
  sleep,
  amountToFineAmount,
  assetDecimals,
  getHubAssetId,
  Asset,
} from 'shared/utils';
import { aliceKeyringPair } from 'shared/polkadot_keyring';
import { DisposableApiPromise, getAssethubApi } from 'shared/utils/substrate';
import { Logger } from 'shared/utils/logger';
import { SubmittableExtrinsic } from '@polkadot/api/types';
import { ISubmittableResult } from '@polkadot/types/types';
import { EventRecord } from '@polkadot/types/interfaces';

let nextAliceNonce: number | undefined;

async function allocateAliceNonce(): Promise<number> {
  const alice = await aliceKeyringPair();
  await using assethub = await getAssethubApi();
  return assethubSigningMutex.runExclusive(async () => {
    if (nextAliceNonce === undefined) {
      nextAliceNonce = (await assethub.rpc.system.accountNextIndex(alice.address)).toNumber();
    }
    return nextAliceNonce++;
  });
}

const MAX_ATTEMPTS = 5;
// The fork-aware tx pool can strand a watched transaction indefinitely on a healthy, authoring
// chain (observed in CI: a tx sat outside any block for 90s, then went Invalid, while its tip-bumped
// resubmission was included within a second). So don't watch a submission forever: if it hasn't
// reached a block by this deadline, give up on it and resubmit.
const INCLUSION_TIMEOUT_MS = 30_000;
// Each retry re-signs the same transfer with a higher tip. sr25519 signatures are
// non-deterministic, so a resubmission is a *replacement* of the (possibly still pooled) previous
// attempt and needs strictly higher priority to be accepted; without the tip bump a retry against
// a still-pooled attempt is rejected with "1014: Priority is too low". The amounts are negligible
// (10000 planck = 1e-6 DOT).
const TIP_STEP = 10_000;

// A permanently failed transfer would leave a nonce gap that strands every higher-nonce transfer
// still waiting to finalize, so before giving up we try to fill the gap with a no-op remark. It
// out-tips all transfer attempts, so it can replace one that is stuck-but-still-pooled; if the
// original transfer wins the race and lands anyway, that's also fine.
async function fillNonceGap(logger: Logger, nonce: number) {
  try {
    const alice = await aliceKeyringPair();
    await using assethub = await getAssethubApi();
    await assethubSigningMutex.runExclusive(async () => {
      await assethub.tx.system
        .remark('bouncer nonce gap fill')
        .signAndSend(alice, { nonce, tip: MAX_ATTEMPTS * TIP_STEP });
    });
  } catch (e) {
    logger.warn(`Failed to fill assethub nonce gap at ${nonce}: ${e}`);
  }
}

// The bouncer owns Alice's Assethub nonces (handed out one at a time under `assethubSigningMutex`),
// so if the on-chain nonce has advanced past `pinnedNonce`, it can only be *our own* transfer for
// that nonce that consumed it — i.e. the transfer landed. This lets us recognize a landed-but-lost
// transfer: after a reorg the inclusion deadline can fire while the tx is being re-included on the
// new fork, and the next resubmission then bounces with "1010 Invalid Transaction: Transaction is
// outdated" (Stale). That stale error is confirmation of success, not a failure.
async function nonceAlreadyConsumed(pinnedNonce: number): Promise<boolean> {
  const alice = await aliceKeyringPair();
  await using assethub = await getAssethubApi();
  return (await assethub.rpc.system.accountNextIndex(alice.address)).toNumber() > pinnedNonce;
}

// How many blocks back to look for a landed-but-lost transaction. It will have been included within
// a block or two of submission, so this is generous.
const RECOVERY_BLOCK_LOOKBACK = 30;

// Once we know one of our submissions for a nonce landed (see `nonceAlreadyConsumed`), the tx is
// sitting in a recent block rather than a state we can `signAndSend`-watch — resubmitting only ever
// yields more "stale" errors. So walk back from the head to find the block containing any of our
// submitted tx hashes and, if an event was expected, read it from that block's events. Returns
// undefined if the tx can't be located within the lookback window.
async function findLandedResult(
  submittedHashes: Set<string>,
  expectedEvent?: { pallet: string; name: string },
): Promise<{ txHash: string; eventData?: unknown } | undefined> {
  await using assethub = await getAssethubApi();
  let blockHash = (await assethub.rpc.chain.getHeader()).hash;
  for (let i = 0; i < RECOVERY_BLOCK_LOOKBACK; i++) {
    const signedBlock = await assethub.rpc.chain.getBlock(blockHash);
    const index = signedBlock.block.extrinsics.findIndex((ex) =>
      submittedHashes.has(ex.hash.toString()),
    );
    if (index >= 0) {
      const txHash = signedBlock.block.extrinsics[index].hash.toString();
      if (expectedEvent === undefined) {
        return { txHash };
      }
      const events = (await (
        await assethub.at(blockHash)
      ).query.system.events()) as unknown as EventRecord[];
      const record = events.find(
        (e) =>
          e.phase.isApplyExtrinsic &&
          e.phase.asApplyExtrinsic.toNumber() === index &&
          e.event.section === expectedEvent.pallet &&
          e.event.method === expectedEvent.name,
      );
      return { txHash, eventData: record?.event.data };
    }
    if (signedBlock.block.header.number.toNumber() === 0) {
      break;
    }
    blockHash = signedBlock.block.header.parentHash;
  }
  return undefined;
}

// the signer is always `//Alice`
export async function submitHubExtrinsic(
  logger: Logger,
  extrinsic: (api: DisposableApiPromise) => SubmittableExtrinsic<'promise', ISubmittableResult>,
  extrinsicName: string,
  expectedEvent?: { pallet: string; name: string },
): Promise<{ txHash: string; eventData?: unknown }> {
  const alice = await aliceKeyringPair();
  await using assethubApi = await getAssethubApi();

  // The nonce is pinned for the lifetime of this transfer and reused by every retry. Retrying
  // with a fresh nonce is what caused double-deposits in the past, when a retracted tx got
  // re-included on a later block alongside its replacement.
  const nonce = await allocateAliceNonce();
  // Every attempt re-signs with a higher tip, so each has a distinct hash. We track them all so a
  // landed-but-lost tx can be located in block history via the nonce-consumed recovery below.
  const submittedHashes = new Set<string>();

  const runSignAndSubmit = async (
    tip: number,
    resolve: (result: { txHash: string; eventData?: unknown }) => void,
    reject: (error: Error) => void,
  ) => {
    const tx = extrinsic(assethubApi);
    const txHash = tx.hash.toString();
    submittedHashes.add(txHash);
    let done = false;
    let inBlock = false;
    let inclusionTimer: ReturnType<typeof setTimeout> | undefined;
    // Assigned when signAndSend resolves; the deadline is only armed after that, and status
    // callbacks only fire once the subscription exists.
    let unsubscribe: () => void;

    // The deadline is armed whenever the transaction is not in a block: on initial submission,
    // and again if the block it made it into is retracted. It is disarmed while the transaction
    // is in a block, so slow finalization doesn't trigger a false retry.
    const armInclusionDeadline = () => {
      clearTimeout(inclusionTimer);
      inclusionTimer = setTimeout(() => {
        if (!done && !inBlock) {
          done = true;
          unsubscribe();
          reject(
            new Error(
              `Transaction not included within ${INCLUSION_TIMEOUT_MS / 1000}s of submission`,
            ),
          );
        }
      }, INCLUSION_TIMEOUT_MS);
    };

    unsubscribe = await tx.signAndSend(alice, { nonce, tip }, (result) => {
      if (result.status.isInBlock || result.status.isFinalized) {
        inBlock = true;
        clearTimeout(inclusionTimer);
      }
      if (result.status.isRetracted) {
        inBlock = false;
        armInclusionDeadline();
      }
      if (result.dispatchError !== undefined) {
        done = true;
        clearTimeout(inclusionTimer);
        if (result.dispatchError.isModule) {
          const decoded = assethubApi.registry.findMetaError(result.dispatchError.asModule);
          const { docs, name, section } = decoded;
          unsubscribe();
          reject(new Error(`${section}.${name}: ${docs.join(' ')}`));
        } else {
          unsubscribe();
          reject(new Error('Error: ' + result.dispatchError.toString()));
        }
      }
      if (result.status.isFinalized) {
        done = true;
        clearTimeout(inclusionTimer);
        unsubscribe();

        if (expectedEvent) {
          const eventData = result.findRecord(expectedEvent.pallet, expectedEvent.name);
          if (eventData === undefined) {
            logger.warn(
              `Error: extrinsic ${extrinsicName} submitted successfully, but expected event ${expectedEvent.pallet}.${expectedEvent.name} was not emitted.`,
            );
          }
          resolve({ txHash, eventData: eventData?.event.data });
        } else {
          resolve({ txHash });
        }
      }
      if (result.status.isInvalid) {
        done = true;
        clearTimeout(inclusionTimer);
        unsubscribe();
        reject(new Error('Transaction is invalid'));
      }
      // Only give up (and resubmit, with the pinned nonce) on terminal states where the tx
      // definitely won't be applied. `isRetracted` is deliberately NOT terminal, but it re-arms
      // the inclusion deadline above so a retracted tx gets a bounded window to re-enter a block.
      if (result.status.isDropped || result.status.isUsurped || result.status.isFinalityTimeout) {
        done = true;
        clearTimeout(inclusionTimer);
        unsubscribe();
        reject(new Error(`Transaction was ${result.status.type.toLowerCase()}`));
      }
    });
    // The callback can fire before signAndSend resolves, so only arm if the transaction isn't
    // already in a block.
    if (!done && !inBlock) {
      armInclusionDeadline();
    }
  };

  for (let attempt = 0; attempt < MAX_ATTEMPTS; attempt++) {
    try {
      return await new Promise<{ txHash: string; eventData?: unknown }>((resolve, reject) => {
        assethubSigningMutex
          .runExclusive(() => runSignAndSubmit(attempt * TIP_STEP, resolve, reject))
          .catch(reject);
      });
    } catch (e) {
      logger.warn(`Error submitting extrinsic ${extrinsicName} (nonce ${nonce}): ${e}`);
      // If our pinned nonce has already been consumed on-chain, one of our own attempts landed, so
      // the operation succeeded despite this attempt failing (typically: reorg → inclusion timeout →
      // stale resubmission). Resubmitting would only yield more "stale" errors, so recover the
      // result — including any expected event — from block history instead of retrying.
      if (await nonceAlreadyConsumed(nonce)) {
        const landed = await findLandedResult(submittedHashes, expectedEvent);
        if (landed !== undefined) {
          logger.debug(
            `Extrinsic ${extrinsicName} (nonce ${nonce}) already applied on-chain; recovered from block history.`,
          );
          return landed;
        }
        // Nonce consumed but the tx wasn't in the lookback window. For a fire-and-forget transfer
        // the consumed nonce is proof enough it landed; for an event-capturing call we error.
        if (expectedEvent === undefined) {
          logger.info(
            `Extrinsic ${extrinsicName} (nonce ${nonce}) already applied on-chain (nonce consumed); treating as success.`,
          );
          return { txHash: submittedHashes.values().next().value ?? '' };
        }
        throw new Error(
          `Extrinsic ${extrinsicName} (nonce ${nonce}) landed on-chain but its ${expectedEvent.pallet}.${expectedEvent.name} event was not found within ${RECOVERY_BLOCK_LOOKBACK} blocks`,
        );
      }
      if (attempt >= MAX_ATTEMPTS - 1) {
        await fillNonceGap(logger, nonce);
        throw e;
      }
      await sleep(2000); // wait before retrying
    }
  }

  // this case is impossible as we throw above
  return {
    txHash: '',
  };
}

export async function sendHubAsset(
  logger: Logger,
  asset: Asset,
  address: string,
  amount: string,
): Promise<string> {
  const planckAmount = parseInt(amountToFineAmount(amount, assetDecimals(asset)));

  let result;
  if (asset === 'HubDot') {
    result = await submitHubExtrinsic(
      logger,
      (api) => api.tx.balances.transferKeepAlive(address, planckAmount),
      `balances.transferKeepAlive(${address}, ${planckAmount})`,
    );
  } else if (asset === 'HubUsdc' || asset === 'HubUsdt') {
    result = await submitHubExtrinsic(
      logger,
      (api) => api.tx.assets.transferKeepAlive(getHubAssetId(asset), address, planckAmount),
      `assets.transferKeepAlive(${asset}, ${address}, ${planckAmount})`,
    );
  } else {
    throw new Error(`Unsupported hub asset type: ${asset}`);
  }
  return result.txHash;
}
