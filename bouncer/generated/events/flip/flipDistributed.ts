import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const flipFlipDistributed = z.object({
  amounts: z.array(z.tuple([accountId, numberOrHex])),
});

export const flipFlipDistributedEvent = defineEvent('Flip.FlipDistributed', flipFlipDistributed);
