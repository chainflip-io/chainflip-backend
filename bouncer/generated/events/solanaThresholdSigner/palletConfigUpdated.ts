import { z } from 'zod';
import { palletCfThresholdSignaturePalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerPalletConfigUpdated = z.object({
  update: palletCfThresholdSignaturePalletConfigUpdate,
});

export const solanaThresholdSignerPalletConfigUpdatedEvent = defineEvent(
  'SolanaThresholdSigner.PalletConfigUpdated',
  solanaThresholdSignerPalletConfigUpdated,
);
