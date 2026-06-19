import { z } from 'zod';
import { palletCfFlipPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const flipPalletConfigUpdated = z.object({ update: palletCfFlipPalletConfigUpdate });

export const flipPalletConfigUpdatedEvent = defineEvent(
  'Flip.PalletConfigUpdated',
  flipPalletConfigUpdated,
);
