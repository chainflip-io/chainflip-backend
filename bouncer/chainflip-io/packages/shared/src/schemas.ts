import { z } from 'zod';
import { Asset } from './enums';
import {
  chainflipAsset,
  chainflipChain,
  hexString,
  numericString,
} from './parsers';

export const quoteQuerySchema = z.object({
  srcAsset: chainflipAsset,
  destAsset: chainflipAsset,
  amount: numericString,
});

export type QuoteQueryParams = z.infer<typeof quoteQuerySchema>;

export const ccmMetadataSchema = z.object({
  gasBudget: z.union([hexString, z.number()]),
  message: z.union([hexString, z.string()]),
  cfParameters: z.union([hexString, z.string()]).optional(),
});

export type CcmMetadata = z.infer<typeof ccmMetadataSchema>;

export const postSwapSchema = z
  .object({
    srcAsset: chainflipAsset,
    destAsset: chainflipAsset,
    srcChain: chainflipChain,
    destChain: chainflipChain,
    destAddress: z.string(),
    amount: numericString,
    ccmMetadata: ccmMetadataSchema.optional(),
  })
  .transform(({ amount, ...rest }) => ({
    ...rest,
    expectedDepositAmount: amount,
  }));

export type SwapRequestBody = z.input<typeof postSwapSchema>;
export type PostSwapResponse = { id: string; depositAddress: string };

export const quoteResponseSchema = z.union([
  z
    .object({
      id: z.string(),
      intermediate_amount: z.string(),
      egress_amount: z.string(),
    })
    .transform(({ id, ...rest }) => ({
      id,
      intermediateAmount: rest.intermediate_amount,
      egressAmount: rest.egress_amount,
    })),
  z
    .object({
      id: z.string(),
      egress_amount: z.string(),
    })
    .transform(({ id, ...rest }) => ({
      id,
      egressAmount: rest.egress_amount,
    })),
]);

export type MarketMakerResponse = z.input<typeof quoteResponseSchema>;
export type QuoteQueryResponse = z.infer<typeof quoteResponseSchema>;

interface BaseRequest {
  id: string; // random UUID
  deposit_amount: string; // base unit of the deposit asset, e.g. wei for ETH
}

interface Intermediate extends BaseRequest {
  source_asset: Exclude<Asset, 'USDC'>;
  intermediate_asset: 'USDC';
  destination_asset: Exclude<Asset, 'USDC'>;
}

interface USDCDeposit extends BaseRequest {
  source_asset: 'USDC';
  intermediate_asset: null;
  destination_asset: Exclude<Asset, 'USDC'>;
}

interface USDCEgress extends BaseRequest {
  source_asset: Exclude<Asset, 'USDC'>;
  intermediate_asset: null;
  destination_asset: 'USDC';
}

export type QuoteRequest = Intermediate | USDCDeposit | USDCEgress;
