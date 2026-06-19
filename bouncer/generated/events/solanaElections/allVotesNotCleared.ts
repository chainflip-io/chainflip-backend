import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const solanaElectionsAllVotesNotCleared = z.null();

export const solanaElectionsAllVotesNotClearedEvent = defineEvent(
  'SolanaElections.AllVotesNotCleared',
  solanaElectionsAllVotesNotCleared,
);
