import { execSync } from 'child_process';
import { getBalanceSync, observeBalanceIncrease } from '../shared/utils';

async function performSwap(SRC_CCY: string, DST_CCY: string, ADDRESS: string) {
    const FEE = 100;
    execSync(`pnpm tsx ./commands/new_swap.ts ${SRC_CCY} ${DST_CCY} ${ADDRESS} ${FEE}`);

    console.log("The args are: " + SRC_CCY + " " + DST_CCY + " " + ADDRESS + " " + FEE);

    let DEPOSIT_ADDRESS_CCY = SRC_CCY;
    if (SRC_CCY == "usdc") {
        DEPOSIT_ADDRESS_CCY = "eth";
    }

    DEPOSIT_ADDRESS_CCY = DEPOSIT_ADDRESS_CCY.trim();

    let SWAP_ADDRESS = execSync(`pnpm tsx ./commands/observe_events.ts --timeout 10000 --succeed_on swapping:SwapDepositAddressReady`);

    SWAP_ADDRESS = JSON.parse(SWAP_ADDRESS);
    SWAP_ADDRESS = SWAP_ADDRESS[0][DEPOSIT_ADDRESS_CCY];

    console.log("The swap address is: " + SWAP_ADDRESS);

    if (SRC_CCY == "btc") {
        console.log("Doing BTC address conversion");
        SWAP_ADDRESS = SWAP_ADDRESS.replace(/^0x/, '');
        SWAP_ADDRESS = Buffer.from(SWAP_ADDRESS, 'hex').toString();
    }

    console.log(`Swap address: ${SWAP_ADDRESS}`);

    const OLD_BALANCE = getBalanceSync(DST_CCY, ADDRESS);

    console.log(`Old balance: ${OLD_BALANCE}`);

    execSync(`pnpm tsx ./commands/fund.ts ${SRC_CCY} ${SWAP_ADDRESS}`);

    console.log("Funded the address");

    execSync(`pnpm tsx ./commands/observe_events.ts --timeout 30000 --succeed_on swapping:SwapExecuted --fail_on foo:bar`);

    console.log("Waiting for balance to update");

    const newBalance = await observeBalanceIncrease(DST_CCY, ADDRESS, OLD_BALANCE);
    console.log(`Swap success! New balance: ${newBalance}!`);
}

async function main() {
    const SRC_CCY = process.argv[2];
    const DST_CCY = process.argv[3];
    const ADDRESS = process.argv[4];
    await performSwap(SRC_CCY, DST_CCY, ADDRESS);
}

main();