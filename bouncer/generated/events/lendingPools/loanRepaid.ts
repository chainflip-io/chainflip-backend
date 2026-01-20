import { z } from 'zod';
import { numberOrHex, palletCfLendingPoolsLoanRepaidActionType } from '../common';

export const lendingPoolsLoanRepaid = z.object({
  loanId: numberOrHex,
  amount: numberOrHex,
  actionType: palletCfLendingPoolsLoanRepaidActionType,
});
