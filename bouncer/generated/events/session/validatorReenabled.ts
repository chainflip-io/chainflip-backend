import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const sessionValidatorReenabled = z.object({ validator: accountId });

export const sessionValidatorReenabledEvent = defineEvent(
  'Session.ValidatorReenabled',
  sessionValidatorReenabled,
);
