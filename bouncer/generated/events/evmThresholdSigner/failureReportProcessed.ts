import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerFailureReportProcessed = z.object({
  requestId: z.number(),
  ceremonyId: numberOrHex,
  reporterId: accountId,
});

export const evmThresholdSignerFailureReportProcessedEvent = defineEvent(
  'EvmThresholdSigner.FailureReportProcessed',
  evmThresholdSignerFailureReportProcessed,
);
