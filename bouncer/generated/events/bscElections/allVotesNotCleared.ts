import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bscElectionsAllVotesNotCleared = z.null();

export const bscElectionsAllVotesNotClearedEvent = defineEvent(
  'BscElections.AllVotesNotCleared',
  bscElectionsAllVotesNotCleared,
);
