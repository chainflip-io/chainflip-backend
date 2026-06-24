import { z } from 'zod';
import { cfChainsEvmSchnorrVerificationComponents, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumBroadcasterBroadcastSuccess = z.object({
  broadcastId: z.number(),
  transactionOutId: cfChainsEvmSchnorrVerificationComponents,
  transactionRef: hexString,
});

export const arbitrumBroadcasterBroadcastSuccessEvent = defineEvent(
  'ArbitrumBroadcaster.BroadcastSuccess',
  arbitrumBroadcasterBroadcastSuccess,
);
