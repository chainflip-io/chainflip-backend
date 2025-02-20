#!/usr/bin/env -S pnpm tsx
import Keyring from '../polkadot/keyring';
import { TestContext } from '../shared/utils/test_context';
import { testEvmDeposits } from '../tests/evm_deposits';

const gennerate_address = false;

if (gennerate_address) {
    const keyring = new Keyring({ type: 'sr25519' });
    const broker = keyring.createFromUri('//BROKER_1');

    console.log(`Broker address : ${broker.address}, Broker public key : ${broker.publicKey}`);
} else {
    await testEvmDeposits(new TestContext());
}

