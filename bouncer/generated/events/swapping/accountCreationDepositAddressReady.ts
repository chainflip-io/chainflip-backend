import { z } from 'zod';
import {
  accountId,
  cfChainsAddressEncodedAddress,
  cfPrimitivesChainsAssetsAnyAsset,
  numberOrHex,
} from '../common';

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
