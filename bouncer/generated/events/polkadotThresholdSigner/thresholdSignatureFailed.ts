import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerThresholdSignatureFailed = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
  offenders: z.array(accountId),
});

export const polkadotThresholdSignerThresholdSignatureFailedEvent = defineEvent(
  'PolkadotThresholdSigner.ThresholdSignatureFailed',
  polkadotThresholdSignerThresholdSignatureFailed,
);
