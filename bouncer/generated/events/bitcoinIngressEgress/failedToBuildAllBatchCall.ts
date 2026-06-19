import { z } from 'zod';
import { cfChainsAllBatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressFailedToBuildAllBatchCall = z.object({
  error: cfChainsAllBatchError,
});

export const bitcoinIngressEgressFailedToBuildAllBatchCallEvent = defineEvent(
  'BitcoinIngressEgress.FailedToBuildAllBatchCall',
  bitcoinIngressEgressFailedToBuildAllBatchCall,
);
