import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const assethubElectionsAllVotesCleared = z.null();

export const assethubElectionsAllVotesClearedEvent = defineEvent(
  'AssethubElections.AllVotesCleared',
  assethubElectionsAllVotesCleared,
);
