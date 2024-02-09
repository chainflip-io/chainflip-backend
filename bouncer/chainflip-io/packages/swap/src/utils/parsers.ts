import { z } from 'zod';
import { hexString } from '@/shared/parsers';

export const thirdPartySwapSchema = z.object({
  uuid: z.string(),
  txHash: hexString,
  txLink: z.string(),
  routeResponse: z
    .object({
      integration: z.enum(['lifi', 'squid']),
    })
    .passthrough(), // pass through routeResponse objects until we are fully certain of its shape
});
