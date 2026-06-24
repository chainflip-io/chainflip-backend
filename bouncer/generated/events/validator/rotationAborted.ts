import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const validatorRotationAborted = z.null();

export const validatorRotationAbortedEvent = defineEvent(
  'Validator.RotationAborted',
  validatorRotationAborted,
);
