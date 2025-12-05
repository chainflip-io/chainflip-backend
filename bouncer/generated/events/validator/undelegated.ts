import { z } from 'zod';
import { accountId, numberOrHex } from '../common';

export const validatorUndelegated = z.object({
  delegator: accountId,
  operator: accountId,
  maxBid: numberOrHex,
});
