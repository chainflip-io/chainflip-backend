import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerKeygenSuccessReported = accountId;

export const solanaThresholdSignerKeygenSuccessReportedEvent = defineEvent(
  'SolanaThresholdSigner.KeygenSuccessReported',
  solanaThresholdSignerKeygenSuccessReported,
);
