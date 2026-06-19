import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const genericElectionsAllVotesCleared = z.null();

export const genericElectionsAllVotesClearedEvent = defineEvent(
  'GenericElections.AllVotesCleared',
  genericElectionsAllVotesCleared,
);
