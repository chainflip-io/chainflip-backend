import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumElectionsAllVotesNotCleared = z.null();

export const arbitrumElectionsAllVotesNotClearedEvent = defineEvent(
  'ArbitrumElections.AllVotesNotCleared',
  arbitrumElectionsAllVotesNotCleared,
);
