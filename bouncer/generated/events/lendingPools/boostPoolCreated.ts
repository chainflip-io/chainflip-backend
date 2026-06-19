import { z } from 'zod';
import { palletCfLendingPoolsBoostBoostPoolId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const lendingPoolsBoostPoolCreated = z.object({
  boostPool: palletCfLendingPoolsBoostBoostPoolId,
});

export const lendingPoolsBoostPoolCreatedEvent = defineEvent(
  'LendingPools.BoostPoolCreated',
  lendingPoolsBoostPoolCreated,
);
