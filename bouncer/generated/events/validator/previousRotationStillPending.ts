import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const validatorPreviousRotationStillPending = z.null();

export const validatorPreviousRotationStillPendingEvent = defineEvent(
  'Validator.PreviousRotationStillPending',
  validatorPreviousRotationStillPending,
);
