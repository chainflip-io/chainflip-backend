import { z } from 'zod';
import { hexString } from '../common';

export const environmentDurableNonceSetForAccount = z.object({
  nonceAccount: hexString,
  durableNonce: hexString,
});
