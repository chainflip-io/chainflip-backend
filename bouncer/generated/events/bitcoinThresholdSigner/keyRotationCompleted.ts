import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerKeyRotationCompleted = z.null();

export const bitcoinThresholdSignerKeyRotationCompletedEvent = defineEvent(
  'BitcoinThresholdSigner.KeyRotationCompleted',
  bitcoinThresholdSignerKeyRotationCompleted,
);
