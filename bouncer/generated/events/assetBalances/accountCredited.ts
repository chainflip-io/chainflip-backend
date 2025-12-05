import { z } from 'zod';
import { accountId, cfPrimitivesChainsAssetsAnyAsset, numberOrHex } from '../common';

export const assetBalancesAccountCredited = z.object({
  accountId,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amountCredited: numberOrHex,
  newBalance: numberOrHex,
});
