import { z } from 'zod';
import { cfChainsAllBatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressFailedToBuildAllBatchCall = z.object({
  error: cfChainsAllBatchError,
});

export const polkadotIngressEgressFailedToBuildAllBatchCallEvent = defineEvent(
  'PolkadotIngressEgress.FailedToBuildAllBatchCall',
  polkadotIngressEgressFailedToBuildAllBatchCall,
);
