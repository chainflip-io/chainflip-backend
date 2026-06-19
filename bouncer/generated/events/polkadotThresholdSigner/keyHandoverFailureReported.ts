import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerKeyHandoverFailureReported = accountId;

export const polkadotThresholdSignerKeyHandoverFailureReportedEvent = defineEvent(
  'PolkadotThresholdSigner.KeyHandoverFailureReported',
  polkadotThresholdSignerKeyHandoverFailureReported,
);
