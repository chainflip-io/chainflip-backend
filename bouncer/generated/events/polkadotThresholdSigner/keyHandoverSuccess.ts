import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotThresholdSignerKeyHandoverSuccess = z.object({ ceremonyId: numberOrHex });

export const polkadotThresholdSignerKeyHandoverSuccessEvent = defineEvent(
  'PolkadotThresholdSigner.KeyHandoverSuccess',
  polkadotThresholdSignerKeyHandoverSuccess,
);
