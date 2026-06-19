import { z } from 'zod';
import {
  accountId,
  hexString,
  spRuntimeDispatchErrorWithPostInfo,
  stateChainRuntimeChainflipEthereumScCallsEthereumSCApi,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const fundingSCCallCannotBeExecuted = z.object({
  caller: accountId,
  scCall: stateChainRuntimeChainflipEthereumScCallsEthereumSCApi,
  callError: spRuntimeDispatchErrorWithPostInfo,
  ethTxHash: hexString,
});

export const fundingSCCallCannotBeExecutedEvent = defineEvent(
  'Funding.SCCallCannotBeExecuted',
  fundingSCCallCannotBeExecuted,
);
