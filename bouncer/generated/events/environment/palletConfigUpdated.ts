import { z } from 'zod';
import { palletCfEnvironmentPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const environmentPalletConfigUpdated = z.object({
  update: palletCfEnvironmentPalletConfigUpdate,
});

export const environmentPalletConfigUpdatedEvent = defineEvent(
  'Environment.PalletConfigUpdated',
  environmentPalletConfigUpdated,
);
