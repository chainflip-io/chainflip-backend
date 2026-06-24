import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const genericElectionsUninitialized = z.null();

export const genericElectionsUninitializedEvent = defineEvent(
  'GenericElections.Uninitialized',
  genericElectionsUninitialized,
);
