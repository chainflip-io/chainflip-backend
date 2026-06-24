import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerKeygenSuccessReported = accountId;

export const polkadotThresholdSignerKeygenSuccessReportedEvent = defineEvent(
  'PolkadotThresholdSigner.KeygenSuccessReported',
  polkadotThresholdSignerKeygenSuccessReported,
);
