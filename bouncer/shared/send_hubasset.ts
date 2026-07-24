import {
  assethubSigningMutex,
  sleep,
  amountToFineAmount,
  assetDecimals,
  getHubAssetId,
  Asset,
} from 'shared/utils';
import { aliceKeyringPair } from 'shared/polkadot_keyring';
import { getAssethubApi } from 'shared/utils/substrate';
import { Logger } from 'shared/utils/logger';

export async function sendHubAsset(
  logger: Logger,
  asset: Asset,
  address: string,
  amount: string,
): Promise<string> {
  const planckAmount = amountToFineAmount(amount, assetDecimals(asset));
  const alice = await aliceKeyringPair();
  await using assethub = await getAssethubApi();

  // Pin the nonce for the lifetime of this transfer. It is fetched lazily under the signing mutex on
  // the first attempt and then reused by every retry. Fetching a fresh nonce per retry is what
  // caused double-deposits when a retracted tx got re-included on a later block.
  let nonce: number | undefined;

  const runSignAndSubmit = async (
    resolve: (txHash: string) => void,
    reject: (error: Error) => void,
  ) => {
    // Fetched under assethubSigningMutex so concurrent senders can't grab the same nonce before
    // this tx enters the pool.
    if (nonce === undefined) {
      nonce = (await assethub.rpc.system.accountNextIndex(alice.address)).toNumber();
    }

    let transferFunction;
    if (asset === 'HubDot') {
      transferFunction = assethub.tx.balances.transferKeepAlive(address, parseInt(planckAmount));
    } else if (asset === 'HubUsdc' || asset === 'HubUsdt') {
      transferFunction = assethub.tx.assets.transferKeepAlive(
        getHubAssetId(asset),
        address,
        parseInt(planckAmount),
      );
    } else {
      throw new Error(`Unsupported hub asset type: ${asset}`);
    }

    const unsubscribe = await transferFunction.signAndSend(alice, { nonce }, (result) => {
      if (result.dispatchError !== undefined) {
        if (result.dispatchError.isModule) {
          const decoded = assethub.registry.findMetaError(result.dispatchError.asModule);
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
        resolve(result.status.hash.toString());
      }
      if (result.status.isInvalid) {
        unsubscribe();
        reject(new Error('Transaction is invalid'));
      }
      // Only give up (and resubmit, with the pinned nonce) on terminal states where the tx
      // definitely won't be applied. `isRetracted` is deliberately NOT terminal.
      if (result.status.isDropped || result.status.isUsurped) {
        unsubscribe();
        reject(new Error(`Transaction was ${result.status.type.toLowerCase()}`));
      }
    });
  };

  for (let retryCount = 0; retryCount < 3; retryCount++) {
    try {
      return await new Promise<string>((resolve, reject) => {
        assethubSigningMutex.runExclusive(() => runSignAndSubmit(resolve, reject)).catch(reject);
      });
    } catch (e) {
      logger.warn(`Error sending asset ${asset} to ${address}: ${e}`);
      if (retryCount >= 2) {
        throw e;
      }
      await sleep(1000); // wait before retrying
    }
  }

  return '';
}
