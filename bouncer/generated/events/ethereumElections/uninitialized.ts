import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumElectionsUninitialized = z.null();

export const ethereumElectionsUninitializedEvent = defineEvent(
  'EthereumElections.Uninitialized',
  ethereumElectionsUninitialized,
);
