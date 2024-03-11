import { Asset  /*, broker */ } from '@chainflip/cli';
import { Keyring } from '@polkadot/api';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { Mutex } from 'async-mutex';
import {
  decodeDotAddressForContract,
  getChainflipApi,
  /* chainFromAsset, */
  shortChainFomAsset,
} from './utils';

const defaultCommissionBps = 100; // 1%
const mutex = new Mutex();

export interface CcmDepositMetadata {
  message: string;
  gasBudget: number;
  cfParameters: string;
}

export async function newSwap(
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  messageMetadata?: CcmDepositMetadata,
  brokerCommissionBps = defaultCommissionBps,
): Promise<void> {
  // const destinationAddress =
  //   destAsset === 'DOT' ? decodeDotAddressForContract(destAddress) : destAddress;
  // const brokerUrl = process.env.BROKER_ENDPOINT || 'http://127.0.0.1:10997';
  // await broker.requestSwapDepositAddress(
  //   {
  //     srcAsset: sourceAsset,
  //     destAsset,
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
  //   },
  // );

  // NOTE: Bypassing broker since it's not supporting Solana yet. Opening channels directly calling the SC.
  await cryptoWaitReady();

  const chainflip = await getChainflipApi();
  const destinationAddress =
    destAsset === 'DOT' ? decodeDotAddressForContract(destAddress) : destAddress;
  const keyring = new Keyring({ type: 'sr25519' });
  const brokerUri = process.env.BROKER_URI ?? '//BROKER_1';
  const broker = keyring.createFromUri(brokerUri);

  const dstChain = shortChainFomAsset(destAsset);

  await mutex.runExclusive(async () => {
    await chainflip.tx.swapping
      .requestSwapDepositAddress(
        sourceAsset,
        destAsset,
        { [dstChain]: destinationAddress },
        brokerCommissionBps,
        messageMetadata && {
          message: messageMetadata.message as `0x${string}`,
          gasBudget: messageMetadata.gasBudget.toString(),
        },
        undefined,
      )
      .signAndSend(broker, { nonce: -1 });
  });
}
