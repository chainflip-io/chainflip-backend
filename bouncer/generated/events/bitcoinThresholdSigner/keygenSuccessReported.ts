import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerKeygenSuccessReported = accountId;

export const bitcoinThresholdSignerKeygenSuccessReportedEvent = defineEvent(
  'BitcoinThresholdSigner.KeygenSuccessReported',
  bitcoinThresholdSignerKeygenSuccessReported,
);
