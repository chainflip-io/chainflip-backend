import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const fundingRedemptionAmountZero = z.object({ accountId });

export const fundingRedemptionAmountZeroEvent = defineEvent(
  'Funding.RedemptionAmountZero',
  fundingRedemptionAmountZero,
);
