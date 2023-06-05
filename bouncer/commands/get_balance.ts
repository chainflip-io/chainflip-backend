import { execSync } from "child_process";

function get_balance(ccy: string, address: string) {
    address = address.trim();
    const result = execSync(`pnpm tsx ./commands/get_${ccy}_balance.ts ${address}`);
    const result_str = result.toString().trim();
    console.log(result_str);
}

const ccy = process.argv[2];
const address = process.argv[3];
get_balance(ccy, address);