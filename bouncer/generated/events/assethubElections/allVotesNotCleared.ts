import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const assethubElectionsAllVotesNotCleared = z.null();

export const assethubElectionsAllVotesNotClearedEvent = defineEvent(
  'AssethubElections.AllVotesNotCleared',
  assethubElectionsAllVotesNotCleared,
);
