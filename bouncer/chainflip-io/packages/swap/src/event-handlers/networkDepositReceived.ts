import { type Chain } from '.prisma/client';
import { encodeAddress } from '@polkadot/util-crypto';
import { z } from 'zod';
import { encodeAddress as encodeBitcoinAddress } from '@/shared/bitcoin';
import {
  u128,
  chainflipAssetEnum,
  hexString,
  DOT_PREFIX,
} from '@/shared/parsers';
import env from '../config/env';
import { Asset } from '../enums';
import logger from '../utils/logger';
import { EventHandlerArgs } from './index';

export const depositReceivedArgs = z
  .object({
    amount: u128,
    asset: chainflipAssetEnum,
    depositAddress: z.union([
      z
        .object({ __kind: z.literal('Taproot'), value: hexString })
        .transform((o) => {
          try {
            return encodeBitcoinAddress(o.value, env.CHAINFLIP_NETWORK);
          } catch {
            return null;
          }
        }),
      hexString,
    ]),
  })
  .refine(
    (args): args is { amount: bigint; asset: Asset; depositAddress: string } =>
      args.depositAddress !== null,
    { message: 'failed to parse bitcoin deposit address' },
  )
  .transform((args) => {
    if (args.asset === 'DOT') {
      return {
        ...args,
        depositAddress: encodeAddress(args.depositAddress, DOT_PREFIX),
      };
    }

    return args;
  });

export type DepositReceivedArgs = z.input<typeof depositReceivedArgs>;

export const networkDepositReceived =
  (chain: Chain) =>
  async ({ prisma, event, block }: EventHandlerArgs) => {
    const { asset, amount, depositAddress } = depositReceivedArgs.parse(
      event.args,
    );

    const channel = await prisma.depositChannel.findFirst({
      where: { depositAddress, srcChain: chain },
      orderBy: { issuedBlock: 'desc' },
    });

    if (!channel || !channel.isSwapping) {
      logger.info('no swap deposit channel found for deposit', {
        block: block.height,
        eventIndexInBlock: event.indexInBlock,
        eventName: event.name,
        depositAddress,
      });

      return;
    }

    const swap = (
      await prisma.swapDepositChannel
        .findFirst({ where: { depositAddress }, orderBy: { id: 'desc' } })
        .swaps({ orderBy: { nativeId: 'desc' } })
    )?.at(0);

    if (!swap) {
      // this happens if the deposit amount is higher than minimum_deposit_amount but smaller than minimum_swap_amount
      logger.warn('no swap found for deposit to swap deposit channel', {
        block: block.height,
        eventIndexInBlock: event.indexInBlock,
        eventName: event.name,
        depositAddress,
      });

      return;
    }

    const ingressFee = amount - BigInt(swap.swapInputAmount.toFixed());

    await prisma.swap.update({
      where: { id: swap.id },
      data: {
        depositAmount: amount.toString(),
        fees: {
          create: { amount: ingressFee.toString(), type: 'INGRESS', asset },
        },
      },
    });
  };
