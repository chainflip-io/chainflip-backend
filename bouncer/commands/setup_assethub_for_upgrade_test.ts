#!/usr/bin/env -S pnpm tsx

import { initializeAssethubChain } from '../shared/initialize_new_chains';
import { getAssethubApi, observeEvent } from '../shared/utils/substrate';
import { createPolkadotVault, rotateAndFund } from './setup_vaults';
import { globalLogger, loggerChild } from '../shared/utils/logger';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { createLpPool } from '../shared/create_lp_pool';
import { depositLiquidity } from '../shared/deposit_liquidity';
import { rangeOrder } from '../shared/range_order';
import { runWithTimeoutAndExit } from '../shared/utils';

async function main(): Promise<void> {
  await using assethub = await getAssethubApi();
  const logger = loggerChild(globalLogger, 'setup_vaults');
  await initializeAssethubChain(logger);
  await submitGovernanceExtrinsic((api) => api.tx.validator.forceRotation());
  const hubActivationRequest = observeEvent(
    logger,
    'assethubVault:AwaitingGovernanceActivation',
  ).event;
  const hubKey = (await hubActivationRequest).data.newPublicKey;
  const { vaultAddress: hubVaultAddress } = await createPolkadotVault(logger, assethub);
  const hubProxyAdded = observeEvent(logger, 'proxy:ProxyAdded', { chain: 'assethub' }).event;
  const [, hubVaultEvent] = await Promise.all([
    rotateAndFund(assethub, hubVaultAddress, hubKey),
    hubProxyAdded,
  ]);
  await submitGovernanceExtrinsic((chainflip) =>
    chainflip.tx.environment.witnessAssethubVaultCreation(hubVaultAddress, {
      blockNumber: hubVaultEvent.block,
      extrinsicIndex: hubVaultEvent.eventIndex,
    }),
  );

  await Promise.all([
    createLpPool(logger, 'HubDot', 10),
    createLpPool(logger, 'HubUsdc', 1),
    createLpPool(logger, 'HubUsdt', 1),
  ]);

  const lp1Deposits = Promise.all([
    depositLiquidity(logger, 'HubDot', 10000, false, '//LP_1'),
    depositLiquidity(logger, 'HubUsdc', 250000, false, '//LP_1'),
    depositLiquidity(logger, 'HubUsdt', 250000, false, '//LP_1'),
  ]);

  const lpApiDeposits = Promise.all([
    depositLiquidity(logger, 'HubDot', 2000, false, '//LP_API'),
    depositLiquidity(logger, 'HubUsdc', 1000, false, '//LP_API'),
    depositLiquidity(logger, 'HubUsdt', 1000, false, '//LP_API'),
  ]);

  await Promise.all([lpApiDeposits, lp1Deposits]);

  await Promise.all([
    rangeOrder(logger, 'HubDot', 10000 * 0.9999),
    rangeOrder(logger, 'HubUsdc', 250000 * 0.9999),
    rangeOrder(logger, 'HubUsdt', 250000 * 0.9999),
  ]);
}

await runWithTimeoutAndExit(main(), 120);
