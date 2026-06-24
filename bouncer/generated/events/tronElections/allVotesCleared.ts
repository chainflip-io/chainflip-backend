import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const tronElectionsAllVotesCleared = z.null();

export const tronElectionsAllVotesClearedEvent = defineEvent(
  'TronElections.AllVotesCleared',
  tronElectionsAllVotesCleared,
);
