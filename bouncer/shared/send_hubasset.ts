import { assethubSigningMutex, sleep, amountToFineAmount, assetDecimals } from './utils';
import { aliceKeyringPair } from './polkadot_keyring';
import { getAssethubApi } from './utils/substrate';

type HubAsset = 'HubUsdc' | 'HubUsdt';
function parseHubAsset(asset: String) : HubAsset {
  switch (asset) {
    case 'HubUsdc':
      return "HubUsdc";
    case 'HubUsdt':
      return "HubUsdt";
    default:
      throw new Error()
  }
}

function getHubAssetId(asset: HubAsset) {
  switch (asset) {
    case 'HubUsdc':
      return 1337;
    case 'HubUsdt':
      return 1984;
  }
}

export async function sendHubAsset(asset: string, address: string, amount: string) {
  const hubAsset = parseHubAsset(asset);
  const planckAmount = amountToFineAmount(amount, assetDecimals(hubAsset));
  const alice = await aliceKeyringPair();
  await using assethub = await getAssethubApi();

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let resolve: any;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let reject: any;
  const promise = new Promise((resolve_, reject_) => {
    resolve = resolve_;
    reject = reject_;
  });

  // Ensure that both of these have been assigned from the callback above
  while (!resolve || !reject) {
    await sleep(1);
  }

  // The mutex ensures that we use the right nonces by eliminating certain
  // race conditions (this doesn't impact performance significantly as
  // waiting for block confirmation can still be done concurrently)
  await assethubSigningMutex.runExclusive(async () => {
    await assethub.tx.assets
      .transferKeepAlive(getHubAssetId(hubAsset), address, parseInt(planckAmount))
      .signAndSend(alice, { nonce: -1 }, ({ status, dispatchError }) => {
        if (dispatchError !== undefined) {
          if (dispatchError.isModule) {
            const decoded = assethub.registry.findMetaError(dispatchError.asModule);
            const { docs, name, section } = decoded;
            reject(new Error(`${section}.${name}: ${docs.join(' ')}`));
          } else {
            reject(new Error('Error: ' + dispatchError.toString()));
          }
        }
        if (status.isInBlock || status.isFinalized) {
          resolve();
        }
      });
  });

  await promise;
}
