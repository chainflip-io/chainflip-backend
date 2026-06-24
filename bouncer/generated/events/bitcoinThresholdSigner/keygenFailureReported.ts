import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerKeygenFailureReported = accountId;

export const bitcoinThresholdSignerKeygenFailureReportedEvent = defineEvent(
  'BitcoinThresholdSigner.KeygenFailureReported',
  bitcoinThresholdSignerKeygenFailureReported,
);
