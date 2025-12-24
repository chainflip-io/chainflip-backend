import { z } from 'zod';
import { numberOrHex } from '../common';

export const lendingPoolsOriginationFeeTaken = z.object({
  loanId: numberOrHex,
  poolFee: numberOrHex,
  networkFee: numberOrHex,
  brokerFee: numberOrHex,
});
