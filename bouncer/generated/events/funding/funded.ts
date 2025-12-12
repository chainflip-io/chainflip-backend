import { z } from 'zod';
import { accountId, cfTraitsFundingSource, numberOrHex } from '../common';

export const fundingFunded = z.object({
  accountId,
  source: cfTraitsFundingSource,
  fundsAdded: numberOrHex,
  totalBalance: numberOrHex,
});
