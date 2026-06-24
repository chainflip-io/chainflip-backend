import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const fundingRedemptionTaxAmountUpdated = z.object({ amount: numberOrHex });

export const fundingRedemptionTaxAmountUpdatedEvent = defineEvent(
  'Funding.RedemptionTaxAmountUpdated',
  fundingRedemptionTaxAmountUpdated,
);
