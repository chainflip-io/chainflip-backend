import { z } from 'zod';
import { numericString, unsignedInteger } from '@/shared/parsers';

export const marketMakerResponseSchema = z.union([
  z
    .object({
      id: z.string(),
      intermediate_amount: numericString,
      egress_amount: numericString,
    })
    .transform(({ id, ...rest }) => ({
      id,
      intermediateAmount: rest.intermediate_amount,
      egressAmount: rest.egress_amount,
    })),
  z
    .object({
      id: z.string(),
      egress_amount: numericString,
    })
    .transform(({ id, ...rest }) => ({
      id,
      egressAmount: rest.egress_amount,
    })),
]);
export type MarketMakerQuote = z.infer<typeof marketMakerResponseSchema>;

export const swapRateResponseSchema = z
  .object({
    // TODO: simplify when we know how Rust `Option` is encoded
    intermediary: unsignedInteger.optional().nullable(),
    output: unsignedInteger,
  })
  .transform((rate) => ({
    intermediateAmount: rate.intermediary?.toString(),
    egressAmount: rate.output.toString(),
  }));
export type BrokerQuote = z.infer<typeof swapRateResponseSchema>;
