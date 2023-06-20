import { Token, getBalance } from "../shared/get_balance";

async function get_balance(ccy: string, address: string) {
    address = address.trim();
    const result = await getBalance(ccy.toUpperCase() as Token, address);
    const result_str = result.toString().trim();
    console.log(result_str);
}

const ccy = process.argv[2];
const address = process.argv[3];
get_balance(ccy, address);