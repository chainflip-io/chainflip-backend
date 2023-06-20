import { ApiPromise, WsProvider } from "@polkadot/api";
import Keyring from "@polkadot/keyring";
import { cryptoWaitReady } from "@polkadot/util-crypto";
import { Mutex } from "async-mutex";

const mutex = new Mutex();

export async function fundDot(address: string, amount: string) {

    const polkadot_endpoint = process.env.POLKADOT_ENDPOINT || 'ws://127.0.0.1:9945';

    let planckAmount: any;
    if (!amount.includes('.')) {
        planckAmount = amount + '0000000000';
    } else {
        const amount_parts = amount.split('.');
        planckAmount = amount_parts[0] + amount_parts[1].padEnd(10, '0').substring(0, 10);
    }
    await cryptoWaitReady();
    const keyring = new Keyring({ type: 'sr25519' });
    const alice = keyring.createFromUri('//Alice');
    const polkadot = await ApiPromise.create({ provider: new WsProvider(polkadot_endpoint), noInitWarn: true });

    let resolve: any;
    let reject: any;
    const promise = new Promise((resolve_, reject_) => {
        resolve = resolve_;
        reject = reject_;
    });

    // The mutex ensures that we use the right nonces by eliminating certain
    // race conditions (this doesn't impact performance significantly as
    // waiting for block confirmation can still be done concurrently)
    await mutex.runExclusive(async () => {

        await polkadot.tx.balances
            .transfer(address, parseInt(planckAmount))
            .signAndSend(alice, { nonce: -1 }, ({ status, dispatchError }) => {
                if (dispatchError !== undefined) {
                    if (dispatchError.isModule) {
                        const decoded = polkadot.registry.findMetaError(dispatchError.asModule);
                        const { docs, name, section } = decoded;
                        reject(new Error(`${section}.${name}: ${docs.join(' ')}`));
                    } else {
                        reject(new Error('Error: ' + dispatchError.toString()));
                    }
                }
                if (status.isInBlock || status.isFinalized) {
                    console.log("finalized!");
                    resolve();
                }
            });
    });


    return promise;

}