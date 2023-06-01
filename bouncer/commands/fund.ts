import { execSync } from 'child_process';

function fund(fund_ccy: string, address: string) {
    if (fund_ccy == "btc") {
        execSync(`pnpm tsx ./commands/fund_btc.ts  ${address} 0.5`);
    } else if (fund_ccy == "eth") {
        execSync(`pnpm tsx ./commands/fund_eth.ts  ${address} 5`);
    } else if (fund_ccy == "dot") {
        execSync(`pnpm tsx ./commands/fund_dot.ts  ${address} 50`);
    } else if (fund_ccy == "usdc") {
        execSync(`pnpm tsx ./commands/fund_usdc.ts ${address} 500`);
    }
}