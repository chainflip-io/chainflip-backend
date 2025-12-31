import { z } from 'zod';
import { accountId, cfPrimitivesChainsAssetsAnyAsset, numberOrHex } from '../common';

export const lendingPoolsLendingFundsRemoved = z.object({
  lenderId: accountId,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  unlockedAmount: numberOrHex,
});
