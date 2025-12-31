import { z } from 'zod';
import { accountId, cfPrimitivesChainsAssetsAnyAsset, numberOrHex } from '../common';

export const lendingPoolsLoanCreated = z.object({
  loanId: numberOrHex,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  borrowerId: accountId,
  principalAmount: numberOrHex,
});
