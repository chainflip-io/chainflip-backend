#!/usr/bin/env -S pnpm tsx
import { newAddress } from '../shared/utils';
import { ExecutableTest } from '../shared/executable_test';
import { requestNewSwap } from '../shared/perform_swap';
import { randomBytes } from 'crypto';
import Keyring from '../polkadot/keyring';

const keyring = new Keyring({ type: 'sr25519' });
const broker = keyring.createFromUri('//BROKER_1');

/* eslint-disable @typescript-eslint/no-use-before-define */
export const spamSolanaDepositChannels = new ExecutableTest(
    'Spam-Solana-Deposit-Channels',
    main,
    1300,
);

// Opens 1000 deposit channels
// Execute: ./commands/run_test.ts spam_solana_deposit_channels.ts
async function main() {
    const amountOfDepositSwapsToOpen = 1000;
    const inputAsset = 'Sol';
    const destAsset = 'Flip';
    const destAddress = await newAddress(destAsset, randomBytes(32).toString('hex'));

    for (let i = 0; i < amountOfDepositSwapsToOpen; i++) {
        console.log(`Opening deposit channel ${i + 1} of ${amountOfDepositSwapsToOpen}`);
        // Wait for 10 seconds to make sure the previous deposit channel is closed
        const swapRequest = await requestNewSwap(
            inputAsset,
            destAsset,
            destAddress,
            'Spam-Solana-Deposit-Channels',
            undefined, // messageMetadata
            0, // brokerCommissionBps
            false, // log
            0, // boostFeeBps
        );
        console.log(`Swap request: ${JSON.stringify(swapRequest)}`);
    }
    console.log(`Opened ${amountOfDepositSwapsToOpen} deposit channels`);
}