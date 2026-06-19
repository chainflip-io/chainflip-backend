import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const flipSlashingPerformed = z.object({ who: accountId, amount: numberOrHex });

export const flipSlashingPerformedEvent = defineEvent(
  'Flip.SlashingPerformed',
  flipSlashingPerformed,
);
