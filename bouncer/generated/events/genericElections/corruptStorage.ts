import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const genericElectionsCorruptStorage = z.null();

export const genericElectionsCorruptStorageEvent = defineEvent(
  'GenericElections.CorruptStorage',
  genericElectionsCorruptStorage,
);
