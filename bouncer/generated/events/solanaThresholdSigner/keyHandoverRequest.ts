import { z } from 'zod';
import { accountId, hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaThresholdSignerKeyHandoverRequest = z.object({
  ceremonyId: numberOrHex,
  fromEpoch: z.number(),
  keyToShare: hexString,
  sharingParticipants: z.array(accountId),
  receivingParticipants: z.array(accountId),
  newKey: hexString,
  toEpoch: z.number(),
});

export const solanaThresholdSignerKeyHandoverRequestEvent = defineEvent(
  'SolanaThresholdSigner.KeyHandoverRequest',
  solanaThresholdSignerKeyHandoverRequest,
);
