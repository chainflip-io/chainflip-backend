import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerKeyRotationCompleted = z.null();

export const polkadotThresholdSignerKeyRotationCompletedEvent = defineEvent(
  'PolkadotThresholdSigner.KeyRotationCompleted',
  polkadotThresholdSignerKeyRotationCompleted,
);
