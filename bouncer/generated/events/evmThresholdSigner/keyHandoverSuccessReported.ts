import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerKeyHandoverSuccessReported = accountId;

export const evmThresholdSignerKeyHandoverSuccessReportedEvent = defineEvent(
  'EvmThresholdSigner.KeyHandoverSuccessReported',
  evmThresholdSignerKeyHandoverSuccessReported,
);
