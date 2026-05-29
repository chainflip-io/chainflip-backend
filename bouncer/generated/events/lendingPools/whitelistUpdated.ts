import { z } from 'zod';
import { palletCfLendingPoolsGeneralLendingWhitelistWhitelistUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const lendingPoolsWhitelistUpdated = z.object({
  update: palletCfLendingPoolsGeneralLendingWhitelistWhitelistUpdate,
});

export const lendingPoolsWhitelistUpdatedEvent = defineEvent(
  'LendingPools.WhitelistUpdated',
  lendingPoolsWhitelistUpdated,
);
