import { z } from 'zod';
import { cfTraitsElectionsElectionInstance, spRuntimeDispatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const environmentElectionInstanceVotesRejected = z.object({
  instance: cfTraitsElectionsElectionInstance,
  error: spRuntimeDispatchError,
});

export const environmentElectionInstanceVotesRejectedEvent = defineEvent(
  'Environment.ElectionInstanceVotesRejected',
  environmentElectionInstanceVotesRejected,
);
