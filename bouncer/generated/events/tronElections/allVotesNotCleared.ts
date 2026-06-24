import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const tronElectionsAllVotesNotCleared = z.null();

export const tronElectionsAllVotesNotClearedEvent = defineEvent(
  'TronElections.AllVotesNotCleared',
  tronElectionsAllVotesNotCleared,
);
