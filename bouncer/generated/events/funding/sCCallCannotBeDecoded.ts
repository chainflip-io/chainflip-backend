import { z } from 'zod';
import { accountId, hexString } from '../common';

export const fundingSCCallCannotBeDecoded = z.object({
  caller: accountId,
  scCallBytes: hexString,
  ethTxHash: hexString,
});
