import { z } from 'zod';
import { accountId, cfChainsBtcAggKey, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinThresholdSignerKeyHandoverRequest = z.object({
  ceremonyId: numberOrHex,
  fromEpoch: z.number(),
  keyToShare: cfChainsBtcAggKey,
  sharingParticipants: z.array(accountId),
  receivingParticipants: z.array(accountId),
  newKey: cfChainsBtcAggKey,
  toEpoch: z.number(),
});

export const bitcoinThresholdSignerKeyHandoverRequestEvent = defineEvent(
  'BitcoinThresholdSigner.KeyHandoverRequest',
  bitcoinThresholdSignerKeyHandoverRequest,
);
