import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerKeygenFailureReported = accountId;

export const polkadotThresholdSignerKeygenFailureReportedEvent = defineEvent(
  'PolkadotThresholdSigner.KeygenFailureReported',
  polkadotThresholdSignerKeygenFailureReported,
);
