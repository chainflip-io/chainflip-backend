import { z } from 'zod';
import {
  accountId,
  cfChainsAddressEncodedAddress,
  cfPrimitivesChainsAssetsAnyAsset,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const liquidityProviderLiquidityDepositAddressReady = z.object({
  channelId: numberOrHex,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  depositAddress: cfChainsAddressEncodedAddress,
  accountId,
  depositChainExpiryBlock: numberOrHex,
  boostFee: z.number(),
  channelOpeningFee: numberOrHex,
});

export const liquidityProviderLiquidityDepositAddressReadyEvent = defineEvent(
  'LiquidityProvider.LiquidityDepositAddressReady',
  liquidityProviderLiquidityDepositAddressReady,
);
