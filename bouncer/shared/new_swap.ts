import { InternalAsset as Asset } from '@chainflip/cli';

import { Keyring } from '@polkadot/api';
import { Mutex } from 'async-mutex';
import { getChainflipApi } from './utils';

const defaultCommissionBps = 100; // 1%

export interface CcmDepositMetadata {
  message: string;
  gasBudget: number;
  cfParameters: string;
}

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
