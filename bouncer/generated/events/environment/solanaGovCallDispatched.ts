import { z } from 'zod';
import { cfChainsSolApiSolanaGovCall } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const environmentSolanaGovCallDispatched = z.object({
  govCall: cfChainsSolApiSolanaGovCall,
  broadcastId: z.number(),
});

export const environmentSolanaGovCallDispatchedEvent = defineEvent(
  'Environment.SolanaGovCallDispatched',
  environmentSolanaGovCallDispatched,
);
