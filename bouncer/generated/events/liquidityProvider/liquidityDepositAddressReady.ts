import { z } from 'zod';
import {
  accountId,
  cfChainsAddressEncodedAddress,
  cfPrimitivesChainsAssetsAnyAsset,
  numberOrHex,
} from '../common';

export const liquidityProviderLiquidityDepositAddressReady = z.object({
  channelId: numberOrHex,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  depositAddress: cfChainsAddressEncodedAddress,
  accountId,
  depositChainExpiryBlock: numberOrHex,
  boostFee: z.number(),
  channelOpeningFee: numberOrHex,
});
