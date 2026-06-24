import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bscElectionsCorruptStorage = z.null();

export const bscElectionsCorruptStorageEvent = defineEvent(
  'BscElections.CorruptStorage',
  bscElectionsCorruptStorage,
);
