import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const fundingRedemptionExpired = z.object({ accountId, txHash: hexString });

export const fundingRedemptionExpiredEvent = defineEvent(
  'Funding.RedemptionExpired',
  fundingRedemptionExpired,
);
