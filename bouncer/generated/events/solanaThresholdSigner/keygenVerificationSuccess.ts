import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerKeygenVerificationSuccess = z.object({ aggKey: hexString });

export const solanaThresholdSignerKeygenVerificationSuccessEvent = defineEvent(
  'SolanaThresholdSigner.KeygenVerificationSuccess',
  solanaThresholdSignerKeygenVerificationSuccess,
);
