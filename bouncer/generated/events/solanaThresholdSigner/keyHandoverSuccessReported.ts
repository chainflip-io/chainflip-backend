import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerKeyHandoverSuccessReported = accountId;

export const solanaThresholdSignerKeyHandoverSuccessReportedEvent = defineEvent(
  'SolanaThresholdSigner.KeyHandoverSuccessReported',
  solanaThresholdSignerKeyHandoverSuccessReported,
);
