import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerKeyHandoverVerificationSuccess = z.object({ aggKey: hexString });

export const solanaThresholdSignerKeyHandoverVerificationSuccessEvent = defineEvent(
  'SolanaThresholdSigner.KeyHandoverVerificationSuccess',
  solanaThresholdSignerKeyHandoverVerificationSuccess,
);
