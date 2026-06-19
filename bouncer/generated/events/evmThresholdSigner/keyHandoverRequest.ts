import { z } from 'zod';
import { accountId, cfChainsEvmAggKey, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const evmThresholdSignerKeyHandoverRequest = z.object({
  ceremonyId: numberOrHex,
  fromEpoch: z.number(),
  keyToShare: cfChainsEvmAggKey,
  sharingParticipants: z.array(accountId),
  receivingParticipants: z.array(accountId),
  newKey: cfChainsEvmAggKey,
  toEpoch: z.number(),
});

export const evmThresholdSignerKeyHandoverRequestEvent = defineEvent(
  'EvmThresholdSigner.KeyHandoverRequest',
  evmThresholdSignerKeyHandoverRequest,
);
