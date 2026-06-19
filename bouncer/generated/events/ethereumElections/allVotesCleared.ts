import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumElectionsAllVotesCleared = z.null();

export const ethereumElectionsAllVotesClearedEvent = defineEvent(
  'EthereumElections.AllVotesCleared',
  ethereumElectionsAllVotesCleared,
);
