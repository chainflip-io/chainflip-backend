import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerFailureReportProcessed = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
  reporterId: accountId,
});

export const bitcoinThresholdSignerFailureReportProcessedEvent = defineEvent(
  'BitcoinThresholdSigner.FailureReportProcessed',
  bitcoinThresholdSignerFailureReportProcessed,
);
