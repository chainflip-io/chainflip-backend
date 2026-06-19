import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerKeyHandoverFailureReported = accountId;

export const solanaThresholdSignerKeyHandoverFailureReportedEvent = defineEvent(
  'SolanaThresholdSigner.KeyHandoverFailureReported',
  solanaThresholdSignerKeyHandoverFailureReported,
);
