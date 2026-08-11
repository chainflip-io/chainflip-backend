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

  const runSignAndSubmit = async (
    tip: number,
    resolve: (result: { txHash: string; eventData?: unknown }) => void,
    reject: (error: Error) => void,
  ) => {
    const tx = extrinsic(assethubApi);
    const txHash = tx.hash.toString();
    const unsubscribe = await tx.signAndSend(alice, { nonce, tip }, (result) => {
      if (result.dispatchError !== undefined) {
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
        unsubscribe();
        reject(new Error('Transaction is invalid'));
      }
      // Only give up (and resubmit, with the pinned nonce) on terminal states where the tx
      // definitely won't be applied. `isRetracted` is deliberately NOT terminal.
      if (result.status.isDropped || result.status.isUsurped || result.status.isFinalityTimeout) {
        unsubscribe();
        reject(new Error(`Transaction was ${result.status.type.toLowerCase()}`));
      }
    });
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
      `transferKeepAlive(${asset}, ${address}, ${planckAmount})`,
    );
  } else {
    throw new Error(`Unsupported hub asset type: ${asset}`);
  }
  return result.txHash;
}
