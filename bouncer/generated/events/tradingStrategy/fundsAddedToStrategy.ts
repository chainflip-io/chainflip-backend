import { z } from 'zod';
import { accountId, cfPrimitivesChainsAssetsAnyAsset, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tradingStrategyFundsAddedToStrategy = z.object({
  strategyId: accountId,
  amounts: z.array(z.tuple([cfPrimitivesChainsAssetsAnyAsset, numberOrHex])),
});

export const tradingStrategyFundsAddedToStrategyEvent = defineEvent(
  'TradingStrategy.FundsAddedToStrategy',
  tradingStrategyFundsAddedToStrategy,
);
