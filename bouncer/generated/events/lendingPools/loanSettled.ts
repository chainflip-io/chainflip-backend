import { z } from 'zod';
import { numberOrHex } from '../common';

export const lendingPoolsLoanSettled = z.object({
  loanId: numberOrHex,
  outstandingPrincipal: numberOrHex,
  viaLiquidation: z.boolean(),
});
