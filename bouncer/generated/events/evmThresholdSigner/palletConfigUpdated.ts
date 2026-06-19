import { z } from 'zod';
import { palletCfThresholdSignaturePalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerPalletConfigUpdated = z.object({
  update: palletCfThresholdSignaturePalletConfigUpdate,
});

export const evmThresholdSignerPalletConfigUpdatedEvent = defineEvent(
  'EvmThresholdSigner.PalletConfigUpdated',
  evmThresholdSignerPalletConfigUpdated,
);
