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

  for (let retryCount = 0; retryCount < 3; retryCount++) {
    try {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      let resolve: any;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      let reject: any;
      const promise = new Promise<string>((resolve_, reject_) => {
        resolve = resolve_;
        reject = reject_;
      });

      // Ensure that both of these have been assigned from the callback above
      while (!resolve || !reject) {
        await sleep(1);
      }

      const runSignAndSubmit = async () => {
        const nonce = await assethub.rpc.system.accountNextIndex(alice.address);

        let transferFunction;
        if (asset === 'HubDot') {
          transferFunction = assethub.tx.balances.transferKeepAlive(
            address,
            parseInt(planckAmount),
          );
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
          if (result.status.isInBlock || result.status.isFinalized) {
            unsubscribe();
            resolve(result.status.hash.toString());
          }
          if (result.status.isInvalid) {
            unsubscribe();
            reject(new Error('Transaction is invalid'));
          }
          if (result.status.isDropped || result.status.isRetracted) {
            unsubscribe();
            reject(new Error('Transaction was dropped or retracted'));
          }
        });
      };

      await assethubSigningMutex.runExclusive(runSignAndSubmit);
      const txHash = await promise;
      return txHash;
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
