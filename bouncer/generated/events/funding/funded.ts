import { z } from 'zod';
import { accountId, cfTraitsFundingSource, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const fundingFunded = z.object({
  accountId,
  source: cfTraitsFundingSource,
  fundsAdded: numberOrHex,
  totalBalance: numberOrHex,
});

export const fundingFundedEvent = defineEvent('Funding.Funded', fundingFunded);
