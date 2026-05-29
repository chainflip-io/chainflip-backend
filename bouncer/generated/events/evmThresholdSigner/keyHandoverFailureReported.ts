import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerKeyHandoverFailureReported = accountId;

export const evmThresholdSignerKeyHandoverFailureReportedEvent = defineEvent(
  'EvmThresholdSigner.KeyHandoverFailureReported',
  evmThresholdSignerKeyHandoverFailureReported,
);
