import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinElectionsAllVotesNotCleared = z.null();

export const bitcoinElectionsAllVotesNotClearedEvent = defineEvent(
  'BitcoinElections.AllVotesNotCleared',
  bitcoinElectionsAllVotesNotCleared,
);
