import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const witnesserCallDispatched = z.object({ callHash: hexString });

export const witnesserCallDispatchedEvent = defineEvent(
  'Witnesser.CallDispatched',
  witnesserCallDispatched,
);
