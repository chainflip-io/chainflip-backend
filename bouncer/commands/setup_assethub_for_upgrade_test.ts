#!/usr/bin/env -S pnpm tsx

import { AddressOrPair } from '@polkadot/api/types';
import { initializeAssethubChain } from 'shared/initialize_new_chains';
import { DisposableApiPromise, getAssethubApi, observeEvent } from 'shared/utils/substrate';
import { globalLogger } from 'shared/utils/logger';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { createLpPool } from 'shared/create_lp_pool';
import { depositLiquidity } from 'shared/deposit_liquidity';
import { rangeOrder } from 'shared/range_order';
import { deferredPromise, handleSubstrateError, runWithTimeoutAndExit } from 'shared/utils';
import { aliceKeyringPair } from 'shared/polkadot_keyring';
import { createPolkadotVault } from 'commands/setup_vaults';
import { newChainflipIO } from 'shared/utils/chainflip_io';

export async function rotateAndFund(
  api: DisposableApiPromise,
  vault: AddressOrPair,
  key: AddressOrPair,
) {
  const { promise, resolve } = deferredPromise<void>();
  const alice = await aliceKeyringPair();
  const rotation = api.tx.proxy.proxy(
    api.createType('MultiAddress', vault),
    null,
    api.tx.utility.batchAll([
      api.tx.proxy.addProxy(
        api.createType('MultiAddress', key),
        api.createType('ProxyType', 'Any'),
        0,
      ),
      api.tx.proxy.removeProxy(
        api.createType('MultiAddress', alice.address),
        api.createType('ProxyType', 'Any'),
        0,
      ),
    ]),
  );

  const nonce = await api.rpc.system.accountNextIndex(alice.address);
  const unsubscribe = await api.tx.utility
    .batchAll([
      // Note the vault needs to be funded before we rotate.
      api.tx.balances.transferKeepAlive(vault, 1000000000000),
      api.tx.balances.transferKeepAlive(key, 1000000000000),
      rotation,
    ])
    .signAndSend(alice, { nonce }, (result) => {
      if (result.isError) {
        handleSubstrateError(api)(result);
      }
      if (result.isFinalized) {
        unsubscribe();
        resolve();
      }
    });

  await promise;
}

async function main(): Promise<void> {
  const cf = (await newChainflipIO(globalLogger, [])).withChildLogger('setup_vaults');
  await using assethub = await getAssethubApi();

  await initializeAssethubChain(cf.logger);
  await submitGovernanceExtrinsic((api) => api.tx.validator.forceRotation());
  const hubActivationRequest = observeEvent(
    cf.logger,
    'assethubVault:AwaitingGovernanceActivation',
  ).event;
  const hubKey = (await hubActivationRequest).data.newPublicKey;
  const { vaultAddress: hubVaultAddress } = await createPolkadotVault(assethub);
  const hubProxyAdded = observeEvent(cf.logger, 'proxy:ProxyAdded', { chain: 'assethub' }).event;
  const [, hubVaultEvent] = await Promise.all([
    rotateAndFund(assethub, hubVaultAddress, hubKey),
    hubProxyAdded,
  ]);
  cf.info('registering assethub vault on state chain');
  await submitGovernanceExtrinsic((chainflip) =>
    chainflip.tx.environment.witnessAssethubVaultCreation(hubVaultAddress, {
      blockNumber: hubVaultEvent.block,
      extrinsicIndex: hubVaultEvent.eventIndex,
    }),
  );

  cf.info('creating pools for assethub assets');
  await cf.all([
    (subcf) => createLpPool(subcf.logger, 'HubDot', 10),
    (subcf) => createLpPool(subcf.logger, 'HubUsdc', 1),
    (subcf) => createLpPool(subcf.logger, 'HubUsdt', 1),
  ]);

  cf.info('funding pools with assethub assets');
  const lp1Deposits = cf.all([
    (subcf) => depositLiquidity(subcf, 'HubDot', 20000, false, '//LP_1'),
    (subcf) => depositLiquidity(subcf, 'HubUsdc', 250000, false, '//LP_1'),
    (subcf) => depositLiquidity(subcf, 'HubUsdt', 250000, false, '//LP_1'),
  ]);

  const lpApiDeposits = cf.all([
    (subcf) => depositLiquidity(subcf, 'HubDot', 20000, false, '//LP_API'),
    (subcf) => depositLiquidity(subcf, 'HubUsdc', 250000, false, '//LP_API'),
    (subcf) => depositLiquidity(subcf, 'HubUsdt', 250000, false, '//LP_API'),
  ]);

  await Promise.all([lpApiDeposits, lp1Deposits]);

  cf.info('creating orders for assethub assets');
  await cf.all([
    (subcf) => rangeOrder(subcf.logger, 'HubDot', 20000 * 0.9999),
    (subcf) => rangeOrder(subcf.logger, 'HubUsdc', 250000 * 0.9999),
    (subcf) => rangeOrder(subcf.logger, 'HubUsdt', 250000 * 0.9999),
  ]);
}

await runWithTimeoutAndExit(main(), 6000);
