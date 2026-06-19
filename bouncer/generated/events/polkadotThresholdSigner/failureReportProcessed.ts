import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerFailureReportProcessed = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
  reporterId: accountId,
});

export const polkadotThresholdSignerFailureReportProcessedEvent = defineEvent(
  'PolkadotThresholdSigner.FailureReportProcessed',
  polkadotThresholdSignerFailureReportProcessed,
);
