import { cryptoWaitReady } from '@polkadot/util-crypto';
import { Keyring } from '@polkadot/api';
import { Asset } from '@chainflip-io/cli/.';
import {
  getChainflipApi,
  encodeDotAddressForContract,
  handleSubstrateError,
  brokerMutex,
} from './utils';

export interface CcmDepositMetadata {
  message: string;
  gas_budget: number;
  cf_parameters: string;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  source_address: any;
}

export async function newSwap(
  sourceToken: Asset,
  destToken: Asset,
  destAddress: string,
  fee: number,
  messageMetadata?: CcmDepositMetadata,
): Promise<void> {
  await cryptoWaitReady();

  const chainflip = await getChainflipApi();
  const destinationAddress =
    destToken === 'DOT' ? encodeDotAddressForContract(destAddress) : destAddress;
  const keyring = new Keyring({ type: 'sr25519' });
  const brokerUri = process.env.BROKER_URI ?? '//BROKER_1';
  const broker = keyring.createFromUri(brokerUri);

  await brokerMutex.runExclusive(async () => {
    await chainflip.tx.swapping
      .requestSwapDepositAddress(
        sourceToken,
        destToken,
        { [destToken === 'USDC' ? 'ETH' : destToken]: destinationAddress },
        fee,
        messageMetadata ?? null,
      )
      .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip));
  });
}
