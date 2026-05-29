import { z } from 'zod';
import { accountId, cfPrimitivesSemVer } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const validatorCFEVersionUpdated = z.object({
  accountId,
  oldVersion: cfPrimitivesSemVer,
  newVersion: cfPrimitivesSemVer,
});

export const validatorCFEVersionUpdatedEvent = defineEvent(
  'Validator.CFEVersionUpdated',
  validatorCFEVersionUpdated,
);
