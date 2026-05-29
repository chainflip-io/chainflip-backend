import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const sessionValidatorDisabled = z.object({ validator: accountId });

export const sessionValidatorDisabledEvent = defineEvent(
  'Session.ValidatorDisabled',
  sessionValidatorDisabled,
);
