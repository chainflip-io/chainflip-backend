import { z } from 'zod';
import { accountId, cfPrimitivesChainsAssetsAnyAsset, numberOrHex } from '../common';

export const liquidityProviderAssetTransferred = z.object({
  from: accountId,
  to: accountId,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amount: numberOrHex,
});
