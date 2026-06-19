import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const genericElectionsAllVotesNotCleared = z.null();

export const genericElectionsAllVotesNotClearedEvent = defineEvent(
  'GenericElections.AllVotesNotCleared',
  genericElectionsAllVotesNotCleared,
);
