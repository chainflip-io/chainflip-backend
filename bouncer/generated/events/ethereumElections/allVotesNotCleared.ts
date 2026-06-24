import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumElectionsAllVotesNotCleared = z.null();

export const ethereumElectionsAllVotesNotClearedEvent = defineEvent(
  'EthereumElections.AllVotesNotCleared',
  ethereumElectionsAllVotesNotCleared,
);
