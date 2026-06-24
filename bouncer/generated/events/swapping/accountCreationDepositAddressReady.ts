import { z } from 'zod';
import {
  accountId,
  cfChainsAddressEncodedAddress,
  cfPrimitivesChainsAssetsAnyAsset,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingAccountCreationDepositAddressReady = z.object({
  channelId: numberOrHex,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  depositAddress: cfChainsAddressEncodedAddress,
  requestedBy: accountId,
  requestedFor: accountId,
  depositChainExpiryBlock: numberOrHex,
  boostFee: z.number(),
  channelOpeningFee: numberOrHex,
  refundAddress: cfChainsAddressEncodedAddress,
});

export const swappingAccountCreationDepositAddressReadyEvent = defineEvent(
  'Swapping.AccountCreationDepositAddressReady',
  swappingAccountCreationDepositAddressReady,
);
