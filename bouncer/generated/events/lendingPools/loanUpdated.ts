import { z } from 'zod';
import { numberOrHex } from '../common';

export const lendingPoolsLoanUpdated = z.object({
  loanId: numberOrHex,
  extraPrincipalAmount: numberOrHex,
});
