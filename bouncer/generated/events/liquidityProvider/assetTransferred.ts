import { z } from 'zod';
import { accountId, cfPrimitivesChainsAssetsAnyAsset, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const liquidityProviderAssetTransferred = z.object({
  from: accountId,
  to: accountId,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amount: numberOrHex,
});

export const liquidityProviderAssetTransferredEvent = defineEvent(
  'LiquidityProvider.AssetTransferred',
  liquidityProviderAssetTransferred,
);
