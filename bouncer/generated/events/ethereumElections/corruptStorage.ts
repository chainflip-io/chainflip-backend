import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumElectionsCorruptStorage = z.null();

export const ethereumElectionsCorruptStorageEvent = defineEvent(
  'EthereumElections.CorruptStorage',
  ethereumElectionsCorruptStorage,
);
