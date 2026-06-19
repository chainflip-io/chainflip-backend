import { z } from 'zod';
import { palletCfSwappingPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingPalletConfigUpdated = z.object({ update: palletCfSwappingPalletConfigUpdate });

export const swappingPalletConfigUpdatedEvent = defineEvent(
  'Swapping.PalletConfigUpdated',
  swappingPalletConfigUpdated,
);
