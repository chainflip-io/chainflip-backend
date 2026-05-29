import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotVaultActivationTxFailedAwaitingGovernance = z.object({
  newPublicKey: hexString,
});

export const polkadotVaultActivationTxFailedAwaitingGovernanceEvent = defineEvent(
  'PolkadotVault.ActivationTxFailedAwaitingGovernance',
  polkadotVaultActivationTxFailedAwaitingGovernance,
);
