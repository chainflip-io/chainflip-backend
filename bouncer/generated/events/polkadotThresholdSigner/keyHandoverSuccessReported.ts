import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerKeyHandoverSuccessReported = accountId;

export const polkadotThresholdSignerKeyHandoverSuccessReportedEvent = defineEvent(
  'PolkadotThresholdSigner.KeyHandoverSuccessReported',
  polkadotThresholdSignerKeyHandoverSuccessReported,
);
