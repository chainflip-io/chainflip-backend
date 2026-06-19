import { z } from 'zod';
import { palletCfPoolsPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const liquidityPoolsPalletConfigUpdated = z.object({
  update: palletCfPoolsPalletConfigUpdate,
});

export const liquidityPoolsPalletConfigUpdatedEvent = defineEvent(
  'LiquidityPools.PalletConfigUpdated',
  liquidityPoolsPalletConfigUpdated,
);
