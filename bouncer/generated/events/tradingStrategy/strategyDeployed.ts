import { z } from 'zod';
import { accountId, palletCfTradingStrategyTradingStrategy } from '../common';

export const tradingStrategyStrategyDeployed = z.object({
  accountId,
  strategyId: accountId,
  strategy: palletCfTradingStrategyTradingStrategy,
});
