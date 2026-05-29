import { z } from 'zod';
import { accountId, hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const fundingFailedFundingAttempt = z.object({
  accountId,
  withdrawalAddress: hexString,
  amount: numberOrHex,
});

export const fundingFailedFundingAttemptEvent = defineEvent(
  'Funding.FailedFundingAttempt',
  fundingFailedFundingAttempt,
);
