import { InternalAsset as Asset, Asset as SCAsset } from '@chainflip/cli';
import {
  decodeDotAddressForContract,
  chainFromAsset,
  stateChainAssetFromAsset,
  getChainflipApi,
} from './utils';

import { cryptoWaitReady } from '@polkadot/util-crypto';
import { Keyring } from '@polkadot/api';
import { Mutex } from 'async-mutex';

const defaultCommissionBps = 100; // 1%

export interface CcmDepositMetadata {
  message: string;
  gasBudget: number;
  cfParameters: string;
}


// await broker.requestSwapDepositAddress(
//   {
//     srcAsset: stateChainAssetFromAsset(sourceAsset) as SCAsset,
//     destAsset: stateChainAssetFromAsset(destAsset) as SCAsset,
//     srcChain: chainFromAsset(sourceAsset),
//     destAddress: destinationAddress,
//     destChain: chainFromAsset(destAsset),
//     ccmMetadata: messageMetadata && {
//       message: messageMetadata.message as `0x${string}`,
//       gasBudget: messageMetadata.gasBudget.toString(),
//     },
//   },
//   {
//     url: brokerUrl,
//     commissionBps: brokerCommissionBps,


const mutex = new Mutex();

export async function newSwap(
  sourceToken: Asset,
  destToken: Asset,
  destAddress: string,
  messageMetadata?: CcmDepositMetadata,
  brokerCommissionBps = defaultCommissionBps,
  boostFeeBps = 0,
): Promise<void> {
  await cryptoWaitReady();

  const chainflip = await getChainflipApi();
  const destinationAddress = destAddress;
  const keyring = new Keyring({ type: 'sr25519' });
  const brokerUri = process.env.BROKER_URI ?? '//BROKER_1';
  const broker = keyring.createFromUri(brokerUri);

  await mutex.runExclusive(async () => {
    await chainflip.tx.swapping
      .requestSwapDepositAddress(
        sourceToken,
        destToken,
        { [destToken === 'Usdc' ? 'Eth' : destToken]: destinationAddress },
        brokerCommissionBps,
        messageMetadata,
        boostFeeBps,
      )
      .signAndSend(broker, { nonce: -1 });
  });
}
