import { z } from 'zod';
import { accountId, numberOrHex, palletCfLendingPoolsBoostBoostPoolId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const lendingPoolsBoostFundsAdded = z.object({
  boosterId: accountId,
  boostPool: palletCfLendingPoolsBoostBoostPoolId,
  amount: numberOrHex,
});

export const lendingPoolsBoostFundsAddedEvent = defineEvent(
  'LendingPools.BoostFundsAdded',
  lendingPoolsBoostFundsAdded,
);
