import { z } from 'zod';
import { u64 } from '@/shared/parsers';
import { encodedAddress } from './common';
import type { EventHandlerArgs } from './index';

const liquidityDepositAddressReadyArgs = z.object({
  channelId: u64,
  depositAddress: encodedAddress,
  // asset: chainflipAsset,
  // depositChainExpiryBlock: u64,
});

export type LiquidityDepositAddressReadyArgs = z.input<
  typeof liquidityDepositAddressReadyArgs
>;

export const liquidityDepositAddressReady = async ({
  prisma,
  event,
  block,
}: EventHandlerArgs) => {
  const { depositAddress, channelId } = liquidityDepositAddressReadyArgs.parse(
    event.args,
  );

  await prisma.depositChannel.create({
    data: {
      channelId,
      issuedBlock: block.height,
      srcChain: depositAddress.chain,
      depositAddress: depositAddress.address,
      isSwapping: false,
    },
  });
};

export default liquidityDepositAddressReady;
