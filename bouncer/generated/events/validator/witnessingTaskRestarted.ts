import { z } from 'zod';
import { accountId, cfPrimitivesWitnessingTaskName } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const validatorWitnessingTaskRestarted = z.object({
  task: cfPrimitivesWitnessingTaskName,
  reporter: accountId,
});

export const validatorWitnessingTaskRestartedEvent = defineEvent(
  'Validator.WitnessingTaskRestarted',
  validatorWitnessingTaskRestarted,
);
