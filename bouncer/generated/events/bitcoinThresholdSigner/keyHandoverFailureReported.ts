import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerKeyHandoverFailureReported = accountId;

export const bitcoinThresholdSignerKeyHandoverFailureReportedEvent = defineEvent(
  'BitcoinThresholdSigner.KeyHandoverFailureReported',
  bitcoinThresholdSignerKeyHandoverFailureReported,
);
