import { z } from 'zod';
import { palletCfThresholdSignaturePalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerPalletConfigUpdated = z.object({
  update: palletCfThresholdSignaturePalletConfigUpdate,
});

export const bitcoinThresholdSignerPalletConfigUpdatedEvent = defineEvent(
  'BitcoinThresholdSigner.PalletConfigUpdated',
  bitcoinThresholdSignerPalletConfigUpdated,
);
