import { z } from 'zod';
import { hexString, numberOrHex } from '../common';

export const grandpaNewAuthorities = z.object({
  authoritySet: z.array(z.tuple([hexString, numberOrHex])),
});
