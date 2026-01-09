import { z } from 'zod';
import { accountId, numberOrHex } from '../common';

export const validatorDelegated = z.object({
  delegator: accountId,
  operator: accountId,
  maxBid: numberOrHex,
});
