import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const fundingMinimumFundingUpdated = z.object({ newMinimum: numberOrHex });

export const fundingMinimumFundingUpdatedEvent = defineEvent(
  'Funding.MinimumFundingUpdated',
  fundingMinimumFundingUpdated,
);
