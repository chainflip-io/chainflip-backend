import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinElectionsAllVotesCleared = z.null();

export const bitcoinElectionsAllVotesClearedEvent = defineEvent(
  'BitcoinElections.AllVotesCleared',
  bitcoinElectionsAllVotesCleared,
);
