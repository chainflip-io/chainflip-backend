import { z } from 'zod';
import {
  accountId,
  hexString,
  spRuntimeDispatchErrorWithPostInfo,
  stateChainRuntimeChainflipEthereumScCallsEthereumSCApi,
} from '../common';

export const fundingSCCallCannotBeExecuted = z.object({
  caller: accountId,
  scCall: stateChainRuntimeChainflipEthereumScCallsEthereumSCApi,
  callError: spRuntimeDispatchErrorWithPostInfo,
  ethTxHash: hexString,
});
