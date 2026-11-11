import { z } from 'zod';
import {
  accountId,
  cfChainsAddressEncodedAddress,
  cfChainsCcmChannelMetadataDecodedCcmAdditionalData,
  cfChainsRefundParametersChannelRefundParametersEncodedAddress,
  cfPrimitivesBeneficiaryAccountId32,
  cfPrimitivesChainsAssetsAnyAsset,
  cfPrimitivesDcaParameters,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingSwapDepositAddressReady = z.object({
  depositAddress: cfChainsAddressEncodedAddress,
  destinationAddress: cfChainsAddressEncodedAddress,
  sourceAsset: cfPrimitivesChainsAssetsAnyAsset,
  destinationAsset: cfPrimitivesChainsAssetsAnyAsset,
  channelId: numberOrHex,
  brokerId: accountId,
  brokerCommissionRate: z.number(),
  channelMetadata: cfChainsCcmChannelMetadataDecodedCcmAdditionalData.nullish(),
  sourceChainExpiryBlock: numberOrHex,
  boostFee: z.number(),
  channelOpeningFee: numberOrHex,
  affiliateFees: z.array(cfPrimitivesBeneficiaryAccountId32),
  refundParameters: cfChainsRefundParametersChannelRefundParametersEncodedAddress,
  dcaParameters: cfPrimitivesDcaParameters.nullish(),
});

export const swappingSwapDepositAddressReadyEvent = defineEvent(
  'Swapping.SwapDepositAddressReady',
  swappingSwapDepositAddressReady,
);
