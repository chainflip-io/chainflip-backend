import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const lendingPoolsLoanUpdated = z.object({
  loanId: numberOrHex,
  extraPrincipalAmount: numberOrHex,
});

export const lendingPoolsLoanUpdatedEvent = defineEvent(
  'LendingPools.LoanUpdated',
  lendingPoolsLoanUpdated,
);
