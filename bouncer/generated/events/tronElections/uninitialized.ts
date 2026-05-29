import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const tronElectionsUninitialized = z.null();

export const tronElectionsUninitializedEvent = defineEvent(
  'TronElections.Uninitialized',
  tronElectionsUninitialized,
);
