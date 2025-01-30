#!/usr/bin/env -S NODE_OPTIONS=--max-old-space-size=6144 pnpm tsx

import { WhaleKeyManager, getEvmEndpoint } from '../shared/utils';
import Web3 from 'web3';
import { formatEther, parseEther } from 'ethers';
import { getBalance } from '../shared/get_balance';

async function testWhaleManager() {
    console.log('Starting WhaleKeyManager test...');
    const web3 = new Web3(getEvmEndpoint('Ethereum'));

    let testAddress: string;
    let testKey: string;
    // Test getting multiple keys
    for (let i = 0; i < 12; i++) {
        console.log(`\nGetting key ${i + 1}:`);
        const startTime = Date.now();
        const key = await WhaleKeyManager.getNextKey();
        console.log(`Key: ${key}`);

        const duration = Date.now() - startTime;

        // Only show first and last 4 characters of the key for security
        const truncatedKey = `${key.slice(0, 6)}...${key.slice(-4)}`;
        console.log(`Key: ${truncatedKey}`);
        console.log(`Time taken: ${duration}ms`);

        // If it's the first key, it should take longer due to initialization
        if (i === 0) {
            console.log('(First key includes initialization time)');
        }

        // Store one of the keys for later testing
        if (i === 0) {
            testKey = key;
            const account = web3.eth.accounts.privateKeyToAccount(key);
            testAddress = account.address;
        }
    }

    // Verify we're cycling through keys by getting a Set of unique keys
    console.log('\nTesting key uniqueness...');
    const uniqueKeys = new Set();
    for (let i = 0; i < 5; i++) {
        const key = await WhaleKeyManager.getNextKey();
        uniqueKeys.add(key);
    }
    console.log(`Number of unique keys: ${uniqueKeys.size}`);
    console.log(`Expected number of unique keys: 10`);
    console.log(`Test ${uniqueKeys.size === 10 ? 'PASSED' : 'FAILED'}`);

    // Test sending funds from one of the whale keys
    console.log('\nTesting fund transfer from whale key...');
    const testKeyForTransfer = await WhaleKeyManager.getNextKey();
    const account = web3.eth.accounts.privateKeyToAccount(testKeyForTransfer);
    const recipientAddress = '0x1234567890123456789012345678901234567890'; // Example address

    // Check initial balance
    const initialBalance = await getBalance('Eth', account.address);
    console.log(`Initial balance: ${initialBalance} Eth`);

    // Send a small amount of ETH
    const tx = {
        from: account.address,
        to: recipientAddress,
        value: parseEther('0.01').toString(),
        gas: 21000,
        gasPrice: await web3.eth.getGasPrice(),
    };

    try {
        const signedTx = await web3.eth.accounts.signTransaction(tx, testKeyForTransfer);
        console.log('Transaction signed successfully');

        const receipt = await web3.eth.sendSignedTransaction(signedTx.rawTransaction!);
        console.log('Transaction sent successfully');
        console.log(`Transaction hash: ${receipt.transactionHash}`);

        // Check final balance
        const finalBalance = await web3.eth.getBalance(account.address);
        console.log(`Final balance: ${finalBalance} Eth`);

        // Verify the transaction was successful
        if (receipt.status) {
            console.log('Transfer test PASSED');
        } else {
            console.log('Transfer test FAILED - transaction reverted');
        }
    } catch (error) {
        console.error('Transfer test FAILED:', error);
        throw error;
    }
}

// Run the test
testWhaleManager()
    .then(() => {
        console.log('\nTest completed successfully');
        process.exit(0);
    })
    .catch((error) => {
        console.error('\nTest failed:', error);
        process.exit(1);
    });