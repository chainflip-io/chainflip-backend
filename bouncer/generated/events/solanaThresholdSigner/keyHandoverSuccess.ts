import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerKeyHandoverSuccess = z.object({ ceremonyId: numberOrHex });

export const solanaThresholdSignerKeyHandoverSuccessEvent = defineEvent(
  'SolanaThresholdSigner.KeyHandoverSuccess',
  solanaThresholdSignerKeyHandoverSuccess,
);
