import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumElectionsCorruptStorage = z.null();

export const arbitrumElectionsCorruptStorageEvent = defineEvent(
  'ArbitrumElections.CorruptStorage',
  arbitrumElectionsCorruptStorage,
);
