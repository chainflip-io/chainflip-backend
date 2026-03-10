import { ChainflipAsset as Asset } from '@chainflip/utils/chainflip';
import { decodeDotAddressForContract, isPolkadotAsset, newAssetAddress } from 'shared/utils';
import { brokerRequestSwapDepositAddress } from 'shared/broker_rpcs';
import { ChainflipIO } from 'shared/utils/chainflip_io';

const defaultCommissionBps = 100; // 1%

export type CcmDepositMetadata = {
  message: string;
  gasBudget: string;
  ccmAdditionalData?: string;
};

export type FillOrKillParamsX128 = {
  retryDurationBlocks: number;
  refundAddress: string;
  minPriceX128: string;
  maxOraclePriceSlippage?: number | null;
  refundCcmMetadata?: CcmDepositMetadata | null;
};

export type DcaParams = {
  numberOfChunks: number;
  chunkIntervalBlocks: number;
};

export async function newSwap<A = []>(
  cf: ChainflipIO<A>,
  sourceAsset: Asset,
  destAsset: Asset,
  destAddress: string,
  messageMetadata?: CcmDepositMetadata,
  brokerCommissionBps = defaultCommissionBps,
  boostFeeBps = 0,
  fillOrKillParams?: FillOrKillParamsX128,
  dcaParams?: DcaParams,
): Promise<void> {
  const destinationAddress = isPolkadotAsset(destAsset)
    ? decodeDotAddressForContract(destAddress)
    : destAddress;
  const defaultRefundAddress = await newAssetAddress(sourceAsset, 'DEFAULT_REFUND');

  const defaultFillOrKillParams: FillOrKillParamsX128 = {
    retryDurationBlocks: 0,
    refundAddress: defaultRefundAddress,
    minPriceX128: '0',
    refundCcmMetadata: undefined,
    maxOraclePriceSlippage: undefined,
  };

  // If the dry_run of the extrinsic fails on the broker-api then it won't retry. So we retry here to
  // avoid flakiness on CI.
  let retryCount = 0;
  while (retryCount < 20) {
    try {
      const fokParams = fillOrKillParams ?? defaultFillOrKillParams;

      const result = await brokerRequestSwapDepositAddress(
        cf.logger,
        sourceAsset,
        destAsset,
        destinationAddress,
        brokerCommissionBps,
        boostFeeBps,
        fokParams,
        messageMetadata,
        dcaParams,
      );

      // set current block height to the block where the deposit channel request was accepted,
      // since calls via the broker API are currently not handled by ChainflipIO, we have to
      // manually update the current block height
      cf.ifYouCallThisYouHaveToRefactor_stepToBlockHeight(result.issued_block);

      break; // Exit the loop on success
    } catch (error) {
      retryCount++;
      cf.error(
        `Request swap deposit address for ${sourceAsset} attempt: ${retryCount} failed: ${error}`,
      );
    }
  }
}
