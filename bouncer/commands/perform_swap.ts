import { execSync } from 'child_process';

async function performSwap(SRC_CCY: string, DST_CCY: string, ADDRESS: string) {
    const FEE = 100;
    execSync(`pnpm tsx ./commands/new_swap.ts ${SRC_CCY} ${DST_CCY} ${ADDRESS} ${FEE}`);

    console.log("The args are: " + SRC_CCY + " " + DST_CCY + " " + ADDRESS + " " + FEE);

    let DEPOSIT_ADDRESS_CCY = SRC_CCY;
    if (SRC_CCY == "usdc") {
        DEPOSIT_ADDRESS_CCY = "eth";
    }

    DEPOSIT_ADDRESS_CCY = DEPOSIT_ADDRESS_CCY.trim();

    let SWAP_ADDRESS = execSync(`pnpm tsx ./commands/observe_events.ts --timeout 10000 --succeed_on swapping:SwapDepositAddressReady --fail_on foo:bar`);

    // SWAP_ADDRESS IS [{"dot":"0x9d8bcece6a43e9c22ea6b7c0143785baca1eb6dd0d7ce2a9a13892d6c0ef6dca"},{"btc":"0x6d7652566d73717542376a324436514b54763151614557577362354342347a443935"},1384] at this point
    SWAP_ADDRESS = JSON.parse(SWAP_ADDRESS);
    SWAP_ADDRESS = SWAP_ADDRESS[0][DEPOSIT_ADDRESS_CCY];

    console.log("The swap address is: " + SWAP_ADDRESS);


    if (SRC_CCY == "btc") {
        console.log("Doing BTC address conversion");
        SWAP_ADDRESS = Buffer.from(SWAP_ADDRESS.toString(), 'hex').toString();
    }
    // SWAP_ADDRESS = SWAP_ADDRESS.toString().trim();
    console.log(`Swap address: ${SWAP_ADDRESS}`);

    let OLD_BALANCE = execSync(`pnpm tsx ./commands/get_balance.ts ${DST_CCY} ${ADDRESS}`);

    console.log(`Old balance: ${OLD_BALANCE}`);

    execSync(`pnpm tsx ./commands/fund.ts ${DST_CCY} ${SWAP_ADDRESS}`);

    execSync(`pnpm tsx ./commands/observe_events.ts --timeout 30000 --succeed_on swapping:SwapExecuted --fail_on foo:bar`);

    console.log("Waiting for balance to update");

    for (let i = 0; i < 60; i++) {
        let NEW_BALANCE = execSync(`pnpm tsx ./commands/get_balance.ts ${DST_CCY} ${ADDRESS}`);

        if (Number(NEW_BALANCE) > Number(OLD_BALANCE)) {
            console.log(`Swap success! New balance: ${NEW_BALANCE}!`);
            return 0;
        } else {
            console.log("Not found yet");
            await new Promise(resolve => setTimeout(resolve, 2000));
        }
    }
    process.exit(1);
}

async function main() {
    let SRC_CCY = process.argv[2];
    let DST_CCY = process.argv[3];
    let ADDRESS = process.argv[4];
    await performSwap(SRC_CCY, DST_CCY, ADDRESS);
}

main();