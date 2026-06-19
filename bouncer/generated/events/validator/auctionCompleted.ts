import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const validatorAuctionCompleted = z.tuple([z.array(accountId), numberOrHex]);

export const validatorAuctionCompletedEvent = defineEvent(
  'Validator.AuctionCompleted',
  validatorAuctionCompleted,
);
