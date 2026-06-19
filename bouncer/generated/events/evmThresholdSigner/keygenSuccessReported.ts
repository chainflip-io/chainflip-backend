import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerKeygenSuccessReported = accountId;

export const evmThresholdSignerKeygenSuccessReportedEvent = defineEvent(
  'EvmThresholdSigner.KeygenSuccessReported',
  evmThresholdSignerKeygenSuccessReported,
);
