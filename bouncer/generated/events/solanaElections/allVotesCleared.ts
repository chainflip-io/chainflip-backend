import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const solanaElectionsAllVotesCleared = z.null();

export const solanaElectionsAllVotesClearedEvent = defineEvent(
  'SolanaElections.AllVotesCleared',
  solanaElectionsAllVotesCleared,
);
