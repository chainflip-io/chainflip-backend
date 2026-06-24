import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerKeyRotationCompleted = z.null();

export const solanaThresholdSignerKeyRotationCompletedEvent = defineEvent(
  'SolanaThresholdSigner.KeyRotationCompleted',
  solanaThresholdSignerKeyRotationCompleted,
);
