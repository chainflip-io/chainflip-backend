import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerKeygenVerificationSuccess = z.object({ aggKey: hexString });

export const polkadotThresholdSignerKeygenVerificationSuccessEvent = defineEvent(
  'PolkadotThresholdSigner.KeygenVerificationSuccess',
  polkadotThresholdSignerKeygenVerificationSuccess,
);
