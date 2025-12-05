import { z } from 'zod';
import {
  accountId,
  hexString,
  stateChainRuntimeChainflipEthereumScCallsEthereumSCApi,
} from '../common';

export const fundingSCCallExecuted = z.object({
  caller: accountId,
  scCall: stateChainRuntimeChainflipEthereumScCallsEthereumSCApi,
  ethTxHash: hexString,
});
