import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const lendingPoolsInterestTaken = z.object({
  loanId: numberOrHex,
  poolInterest: numberOrHex,
  networkInterest: numberOrHex,
  brokerInterest: numberOrHex,
  lowLtvPenalty: numberOrHex,
});

export const lendingPoolsInterestTakenEvent = defineEvent(
  'LendingPools.InterestTaken',
  lendingPoolsInterestTaken,
);
