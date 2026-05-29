import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerThresholdSignatureFailed = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
  offenders: z.array(accountId),
});

export const bitcoinThresholdSignerThresholdSignatureFailedEvent = defineEvent(
  'BitcoinThresholdSigner.ThresholdSignatureFailed',
  bitcoinThresholdSignerThresholdSignatureFailed,
);
