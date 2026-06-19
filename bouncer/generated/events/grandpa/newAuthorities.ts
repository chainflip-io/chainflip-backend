import { z } from 'zod';
import { hexString, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const grandpaNewAuthorities = z.object({
  authoritySet: z.array(z.tuple([hexString, numberOrHex])),
});

export const grandpaNewAuthoritiesEvent = defineEvent(
  'Grandpa.NewAuthorities',
  grandpaNewAuthorities,
);
