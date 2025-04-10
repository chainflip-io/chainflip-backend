#!/usr/bin/env -S pnpm tsx

import { AddressOrPair } from '@polkadot/api/types';
import { initializeAssethubChain } from '../shared/initialize_new_chains';
import { DisposableApiPromise, getAssethubApi, observeEvent } from '../shared/utils/substrate';
import { globalLogger, loggerChild, Logger } from '../shared/utils/logger';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import { createLpPool } from '../shared/create_lp_pool';
import { depositLiquidity } from '../shared/deposit_liquidity';
import { rangeOrder } from '../shared/range_order';
import { deferredPromise, handleSubstrateError, runWithTimeoutAndExit } from '../shared/utils';
import { aliceKeyringPair } from '../shared/polkadot_keyring';

export async function createAssethubVault(logger: Logger, api: DisposableApiPromise) {
  const { promise, resolve } = deferredPromise<{
    vaultAddress: AddressOrPair;
    vaultExtrinsicIndex: number;
  }>();

  const alice = await aliceKeyringPair();
  const unsubscribe = await api.tx.proxy
    .createPure(api.createType('ProxyType', 'Any'), 0, 0)
    .signAndSend(alice, { nonce: -1 }, (result) => {
      if (result.isError) {
        handleSubstrateError(api)(result);
      }
      if (result.isInBlock) {
        logger.info('Assethub Vault created');
        // TODO: figure out type inference so we don't have to coerce using `any`
        const pureCreated = result.findRecord('proxy', 'PureCreated')!;
        resolve({
          vaultAddress: pureCreated.event.data[0] as AddressOrPair,
          vaultExtrinsicIndex: result.txIndex!,
        });
        unsubscribe();
      }
    });

  return promise;
}

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

  const unsubscribe = await api.tx.utility
    .batchAll([
      // Note the vault needs to be funded before we rotate.
      api.tx.balances.transferKeepAlive(vault, 1000000000000),
      api.tx.balances.transferKeepAlive(key, 1000000000000),
      rotation,
    ])
    .signAndSend(alice, { nonce: -1 }, (result) => {
      if (result.isError) {
        handleSubstrateError(api)(result);
      }
      if (result.isInBlock) {
        unsubscribe();
        resolve();
      }
    });

  await promise;
}

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
  const { vaultAddress: hubVaultAddress } = await createAssethubVault(logger, assethub);
  const hubProxyAdded = observeEvent(logger, 'proxy:ProxyAdded', { chain: 'assethub' }).event;
  const [, hubVaultEvent] = await Promise.all([
    rotateAndFund(assethub, hubVaultAddress, hubKey),
    hubProxyAdded,
  ]);
  logger.info('registering assethub vault on state chain');
  await submitGovernanceExtrinsic((chainflip) =>
    chainflip.tx.environment.witnessAssethubVaultCreation(hubVaultAddress, {
      blockNumber: hubVaultEvent.block,
      extrinsicIndex: hubVaultEvent.eventIndex,
    }),
  );

  logger.info('creating pools for assethub assets');
  await Promise.all([
    createLpPool(logger, 'HubDot', 10),
    createLpPool(logger, 'HubUsdc', 1),
    createLpPool(logger, 'HubUsdt', 1),
  ]);

  logger.info('funding pools with assethub assets');
  const lp1Deposits = Promise.all([
    depositLiquidity(logger, 'HubDot', 100000, false, '//LP_1'),
    depositLiquidity(logger, 'HubUsdc', 2500000, false, '//LP_1'),
    depositLiquidity(logger, 'HubUsdt', 2500000, false, '//LP_1'),
  ]);

  const lpApiDeposits = Promise.all([
    depositLiquidity(logger, 'HubDot', 200000, false, '//LP_API'),
    depositLiquidity(logger, 'HubUsdc', 2500000, false, '//LP_API'),
    depositLiquidity(logger, 'HubUsdt', 2500000, false, '//LP_API'),
  ]);

  await Promise.all([lpApiDeposits, lp1Deposits]);

  logger.info('creating orders for assethub assets');
  await Promise.all([
    rangeOrder(logger, 'HubDot', 100000 * 0.9999),
    rangeOrder(logger, 'HubUsdc', 2500000 * 0.9999),
    rangeOrder(logger, 'HubUsdt', 2500000 * 0.9999),
  ]);
}

await runWithTimeoutAndExit(main(), 6000);
