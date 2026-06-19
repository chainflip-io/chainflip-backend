import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerKeyHandoverFailure = z.object({ ceremonyId: numberOrHex });

export const solanaThresholdSignerKeyHandoverFailureEvent = defineEvent(
  'SolanaThresholdSigner.KeyHandoverFailure',
  solanaThresholdSignerKeyHandoverFailure,
);
