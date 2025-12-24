import { z } from 'zod';
import { accountId, cfPrimitivesChainsAssetsAnyAsset, numberOrHex } from '../common';

export const tradingStrategyFundsAddedToStrategy = z.object({
  strategyId: accountId,
  amounts: z.array(z.tuple([cfPrimitivesChainsAssetsAnyAsset, numberOrHex])),
});
