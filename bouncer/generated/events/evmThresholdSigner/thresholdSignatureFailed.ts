import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerThresholdSignatureFailed = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
  offenders: z.array(accountId),
});

export const evmThresholdSignerThresholdSignatureFailedEvent = defineEvent(
  'EvmThresholdSigner.ThresholdSignatureFailed',
  evmThresholdSignerThresholdSignatureFailed,
);
