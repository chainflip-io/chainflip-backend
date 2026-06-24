import { z } from 'zod';
import { accountId, hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const fundingRedemptionSettled = z.object({
  accountId,
  amount: numberOrHex,
  txHash: hexString,
});

export const fundingRedemptionSettledEvent = defineEvent(
  'Funding.RedemptionSettled',
  fundingRedemptionSettled,
);
