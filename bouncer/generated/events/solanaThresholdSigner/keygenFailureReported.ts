import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerKeygenFailureReported = accountId;

export const solanaThresholdSignerKeygenFailureReportedEvent = defineEvent(
  'SolanaThresholdSigner.KeygenFailureReported',
  solanaThresholdSignerKeygenFailureReported,
);
