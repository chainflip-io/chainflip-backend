import { z } from 'zod';
import {
  cfPrimitivesBeneficiaryAccountId32,
  cfPrimitivesChainsAssetsAnyAsset,
  numberOrHex,
  palletCfLendingPoolsGeneralLendingLoanType,
} from '../common';

export const lendingPoolsLoanCreated = z.object({
  loanId: numberOrHex,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  loanType: palletCfLendingPoolsGeneralLendingLoanType,
  principalAmount: numberOrHex,
  broker: cfPrimitivesBeneficiaryAccountId32.nullish(),
});
