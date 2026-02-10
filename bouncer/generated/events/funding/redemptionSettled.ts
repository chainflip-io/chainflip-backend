import { z } from 'zod';
import { accountId, hexString, numberOrHex } from '../common';

export const fundingRedemptionSettled = z.object({
  accountId,
  amount: numberOrHex,
  txHash: hexString,
});
