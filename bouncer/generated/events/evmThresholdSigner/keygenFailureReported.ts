import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerKeygenFailureReported = accountId;

export const evmThresholdSignerKeygenFailureReportedEvent = defineEvent(
  'EvmThresholdSigner.KeygenFailureReported',
  evmThresholdSignerKeygenFailureReported,
);
