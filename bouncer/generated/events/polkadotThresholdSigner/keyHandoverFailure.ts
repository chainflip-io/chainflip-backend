import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerKeyHandoverFailure = z.object({ ceremonyId: numberOrHex });

export const polkadotThresholdSignerKeyHandoverFailureEvent = defineEvent(
  'PolkadotThresholdSigner.KeyHandoverFailure',
  polkadotThresholdSignerKeyHandoverFailure,
);
