import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerKeyHandoverSuccessReported = accountId;

export const bitcoinThresholdSignerKeyHandoverSuccessReportedEvent = defineEvent(
  'BitcoinThresholdSigner.KeyHandoverSuccessReported',
  bitcoinThresholdSignerKeyHandoverSuccessReported,
);
