import { z } from 'zod';
import {
  accountId,
  cfPrimitivesChainsAssetsAnyAsset,
  numberOrHex,
  spRuntimeDispatchError,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const liquidityProviderAssetBalancePurgeFailed = z.object({
  accountId,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amount: numberOrHex,
  error: spRuntimeDispatchError,
});

export const liquidityProviderAssetBalancePurgeFailedEvent = defineEvent(
  'LiquidityProvider.AssetBalancePurgeFailed',
  liquidityProviderAssetBalancePurgeFailed,
);
