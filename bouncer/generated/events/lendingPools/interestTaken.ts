import { z } from 'zod';
import { numberOrHex } from '../common';

export const lendingPoolsInterestTaken = z.object({
  loanId: numberOrHex,
  poolInterest: numberOrHex,
  networkInterest: numberOrHex,
  brokerInterest: numberOrHex,
  lowLtvPenalty: numberOrHex,
});
