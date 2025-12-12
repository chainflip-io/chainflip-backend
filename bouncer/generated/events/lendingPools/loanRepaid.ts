import { z } from 'zod';
import { numberOrHex } from '../common';

export const lendingPoolsLoanRepaid = z.object({ loanId: numberOrHex, amount: numberOrHex });
