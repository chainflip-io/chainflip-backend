import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const lendingPoolsOriginationFeeTaken = z.object({
  loanId: numberOrHex,
  poolFee: numberOrHex,
  networkFee: numberOrHex,
  brokerFee: numberOrHex,
});

export const lendingPoolsOriginationFeeTakenEvent = defineEvent(
  'LendingPools.OriginationFeeTaken',
  lendingPoolsOriginationFeeTaken,
);
