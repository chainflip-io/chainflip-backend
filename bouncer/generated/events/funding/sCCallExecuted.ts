import { z } from 'zod';
import {
  accountId,
  hexString,
  stateChainRuntimeChainflipEthereumScCallsEthereumSCApi,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const fundingSCCallExecuted = z.object({
  caller: accountId,
  scCall: stateChainRuntimeChainflipEthereumScCallsEthereumSCApi,
  ethTxHash: hexString,
});

export const fundingSCCallExecutedEvent = defineEvent(
  'Funding.SCCallExecuted',
  fundingSCCallExecuted,
);
