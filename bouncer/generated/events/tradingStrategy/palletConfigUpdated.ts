import { z } from 'zod';
import { palletCfTradingStrategyPalletConfigUpdate } from '../common';

export const tradingStrategyPalletConfigUpdated = z.object({
  update: palletCfTradingStrategyPalletConfigUpdate,
});
