import { z } from 'zod';
import { palletCfTradingStrategyPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tradingStrategyPalletConfigUpdated = z.object({
  update: palletCfTradingStrategyPalletConfigUpdate,
});

export const tradingStrategyPalletConfigUpdatedEvent = defineEvent(
  'TradingStrategy.PalletConfigUpdated',
  tradingStrategyPalletConfigUpdated,
);
