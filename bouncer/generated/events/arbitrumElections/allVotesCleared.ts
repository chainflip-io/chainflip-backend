import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumElectionsAllVotesCleared = z.null();

export const arbitrumElectionsAllVotesClearedEvent = defineEvent(
  'ArbitrumElections.AllVotesCleared',
  arbitrumElectionsAllVotesCleared,
);
