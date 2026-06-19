import { z } from 'zod';
import { palletCfThresholdSignaturePalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerPalletConfigUpdated = z.object({
  update: palletCfThresholdSignaturePalletConfigUpdate,
});

export const polkadotThresholdSignerPalletConfigUpdatedEvent = defineEvent(
  'PolkadotThresholdSigner.PalletConfigUpdated',
  polkadotThresholdSignerPalletConfigUpdated,
);
