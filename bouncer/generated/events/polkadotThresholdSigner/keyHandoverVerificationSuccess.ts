import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerKeyHandoverVerificationSuccess = z.object({
  aggKey: hexString,
});

export const polkadotThresholdSignerKeyHandoverVerificationSuccessEvent = defineEvent(
  'PolkadotThresholdSigner.KeyHandoverVerificationSuccess',
  polkadotThresholdSignerKeyHandoverVerificationSuccess,
);
