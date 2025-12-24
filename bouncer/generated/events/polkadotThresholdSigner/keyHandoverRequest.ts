import { z } from 'zod';
import { accountId, hexString, numberOrHex } from '../common';

export const polkadotThresholdSignerKeyHandoverRequest = z.object({
  ceremonyId: numberOrHex,
  fromEpoch: z.number(),
  keyToShare: hexString,
  sharingParticipants: z.array(accountId),
  receivingParticipants: z.array(accountId),
  newKey: hexString,
  toEpoch: z.number(),
});
