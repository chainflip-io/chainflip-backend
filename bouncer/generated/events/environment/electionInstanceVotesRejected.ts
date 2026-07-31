import { z } from 'zod';
import { spRuntimeDispatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const environmentElectionInstanceVotesRejected = z.object({
  instance: z.number(),
  error: spRuntimeDispatchError,
});

export const environmentElectionInstanceVotesRejectedEvent = defineEvent(
  'Environment.ElectionInstanceVotesRejected',
  environmentElectionInstanceVotesRejected,
);
