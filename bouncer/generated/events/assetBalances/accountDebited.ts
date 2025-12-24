import { z } from 'zod';
import { accountId, cfPrimitivesChainsAssetsAnyAsset, numberOrHex } from '../common';

export const assetBalancesAccountDebited = z.object({
  accountId,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amountDebited: numberOrHex,
  newBalance: numberOrHex,
});
