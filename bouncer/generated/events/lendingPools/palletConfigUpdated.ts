import { z } from 'zod';
import { palletCfLendingPoolsPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const lendingPoolsPalletConfigUpdated = z.object({
  update: palletCfLendingPoolsPalletConfigUpdate,
});

export const lendingPoolsPalletConfigUpdatedEvent = defineEvent(
  'LendingPools.PalletConfigUpdated',
  lendingPoolsPalletConfigUpdated,
);
