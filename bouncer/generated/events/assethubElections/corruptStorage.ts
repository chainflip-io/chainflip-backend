import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const assethubElectionsCorruptStorage = z.null();

export const assethubElectionsCorruptStorageEvent = defineEvent(
  'AssethubElections.CorruptStorage',
  assethubElectionsCorruptStorage,
);
