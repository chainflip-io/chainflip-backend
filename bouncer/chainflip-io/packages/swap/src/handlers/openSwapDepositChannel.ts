import { z } from 'zod';
import * as broker from '@/shared/broker';
import { openSwapDepositChannelSchema } from '@/shared/schemas';
import { validateAddress } from '@/shared/validation/addressValidation';
import prisma from '../client';
import env from '../config/env';
import { calculateExpiryTime } from '../utils/function';
import { validateSwapAmount } from '../utils/rpc';
import screenAddress from '../utils/screenAddress';
import ServiceError from '../utils/ServiceError';

export default async function openSwapDepositChannel(
  input: z.output<typeof openSwapDepositChannelSchema>,
) {
  if (
    !validateAddress(input.destChain, input.destAddress, env.CHAINFLIP_NETWORK)
  ) {
    throw ServiceError.badRequest('provided address is not valid');
  }

  if (await screenAddress(input.destAddress)) {
    throw ServiceError.badRequest('provided address is sanctioned');
  }

  const result = await validateSwapAmount(
    { asset: input.srcAsset, chain: input.srcChain },
    BigInt(input.expectedDepositAmount),
  );

  if (!result.success) throw ServiceError.badRequest(result.reason);

  const {
    address: depositAddress,
    sourceChainExpiryBlock: srcChainExpiryBlock,
    ...blockInfo
  } = await broker.requestSwapDepositAddress(
    input,
    { url: env.RPC_BROKER_HTTPS_URL, commissionBps: 0 },
    env.CHAINFLIP_NETWORK,
  );

  const { destChain, ccmMetadata, ...rest } = input;

  const chainInfo = await prisma.chainTracking.findFirst({
    where: {
      chain: input.srcChain,
    },
  });
  const estimatedExpiryTime = calculateExpiryTime({
    chainInfo,
    expiryBlock: srcChainExpiryBlock,
  });

  const {
    issuedBlock,
    srcChain,
    channelId,
    depositAddress: channelDepositAddress,
    brokerCommissionBps,
  } = await prisma.swapDepositChannel.upsert({
    where: {
      issuedBlock_srcChain_channelId: {
        channelId: blockInfo.channelId,
        issuedBlock: blockInfo.issuedBlock,
        srcChain: input.srcChain,
      },
    },
    create: {
      ...rest,
      depositAddress,
      srcChainExpiryBlock,
      estimatedExpiryAt: estimatedExpiryTime,
      ccmGasBudget: ccmMetadata?.gasBudget,
      ccmMessage: ccmMetadata?.message,
      brokerCommissionBps: 0,
      openedThroughBackend: true,
      ...blockInfo,
    },
    update: {
      openedThroughBackend: true,
    },
  });

  return {
    id: `${issuedBlock}-${srcChain}-${channelId}`,
    depositAddress: channelDepositAddress,
    brokerCommissionBps,
    issuedBlock,
    srcChainExpiryBlock,
    estimatedExpiryTime: estimatedExpiryTime?.valueOf(),
  };
}
