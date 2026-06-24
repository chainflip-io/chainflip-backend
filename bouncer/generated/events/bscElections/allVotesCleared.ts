import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bscElectionsAllVotesCleared = z.null();

export const bscElectionsAllVotesClearedEvent = defineEvent(
  'BscElections.AllVotesCleared',
  bscElectionsAllVotesCleared,
);
