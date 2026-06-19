import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const validatorStoppedBidding = z.object({ accountId });

export const validatorStoppedBiddingEvent = defineEvent(
  'Validator.StoppedBidding',
  validatorStoppedBidding,
);
