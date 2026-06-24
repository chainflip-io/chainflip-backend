import { z } from 'zod';
import { accountId, palletCfTradingStrategyTradingStrategy } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tradingStrategyStrategyDeployed = z.object({
  accountId,
  strategyId: accountId,
  strategy: palletCfTradingStrategyTradingStrategy,
});

export const tradingStrategyStrategyDeployedEvent = defineEvent(
  'TradingStrategy.StrategyDeployed',
  tradingStrategyStrategyDeployed,
);
