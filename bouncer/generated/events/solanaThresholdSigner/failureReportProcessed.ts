import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerFailureReportProcessed = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
  reporterId: accountId,
});

export const solanaThresholdSignerFailureReportProcessedEvent = defineEvent(
  'SolanaThresholdSigner.FailureReportProcessed',
  solanaThresholdSignerFailureReportProcessed,
);
