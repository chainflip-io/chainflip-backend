import { z } from 'zod';
import { cfChainsBtcAggKey } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinVaultActivationTxFailedAwaitingGovernance = z.object({
  newPublicKey: cfChainsBtcAggKey,
});

export const bitcoinVaultActivationTxFailedAwaitingGovernanceEvent = defineEvent(
  'BitcoinVault.ActivationTxFailedAwaitingGovernance',
  bitcoinVaultActivationTxFailedAwaitingGovernance,
);
