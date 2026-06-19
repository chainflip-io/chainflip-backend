import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const validatorStartedBidding = z.object({ accountId });

export const validatorStartedBiddingEvent = defineEvent(
  'Validator.StartedBidding',
  validatorStartedBidding,
);
