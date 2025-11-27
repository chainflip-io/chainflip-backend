import { InternalAsset as Asset, broker } from '@chainflip/cli';
import {
  decodeDotAddressForContract,
  stateChainAssetFromAsset,
  isPolkadotAsset,
  newAssetAddress,
} from 'shared/utils';
import { Logger } from 'shared/utils/logger';
import { brokerApiEndpoint } from 'shared/json_rpc';

const defaultCommissionBps = 100; // 1%

type RequestDepositChannelParams = Parameters<(typeof broker)['requestSwapDepositAddress']>[0];

export type CcmDepositMetadata = NonNullable<RequestDepositChannelParams['ccmParams']>;

export type FillOrKillParamsX128 = NonNullable<RequestDepositChannelParams['fillOrKillParams']>;
export type DcaParams = NonNullable<RequestDepositChannelParams['dcaParams']>;

export async function newSwap(
  logger: Logger,
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
      await broker.requestSwapDepositAddress(
        {
          srcAsset: stateChainAssetFromAsset(sourceAsset),
          destAsset: stateChainAssetFromAsset(destAsset),
          destAddress: destinationAddress,
          ccmParams: messageMetadata && {
            message: messageMetadata.message,
            gasBudget: messageMetadata.gasBudget.toString(),
            ccmAdditionalData: messageMetadata.ccmAdditionalData,
          },
          commissionBps: brokerCommissionBps,
          maxBoostFeeBps: boostFeeBps,
          fillOrKillParams: fillOrKillParams || defaultFillOrKillParams,
          dcaParams,
        },
        {
          url: brokerApiEndpoint,
        },
        'backspin',
      );
      break; // Exit the loop on success
    } catch (error) {
      retryCount++;
      logger.error(
        `Request swap deposit address for ${sourceAsset} attempt: ${retryCount} failed: ${error}`,
      );
    }
  }
}
