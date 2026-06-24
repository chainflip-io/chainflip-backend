import { z } from 'zod';
import { palletCfValidatorPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const validatorPalletConfigUpdated = z.object({
  update: palletCfValidatorPalletConfigUpdate,
});

export const validatorPalletConfigUpdatedEvent = defineEvent(
  'Validator.PalletConfigUpdated',
  validatorPalletConfigUpdated,
);
