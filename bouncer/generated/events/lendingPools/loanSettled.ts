import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const lendingPoolsLoanSettled = z.object({
  loanId: numberOrHex,
  outstandingPrincipal: numberOrHex,
  viaLiquidation: z.boolean(),
});

export const lendingPoolsLoanSettledEvent = defineEvent(
  'LendingPools.LoanSettled',
  lendingPoolsLoanSettled,
);
