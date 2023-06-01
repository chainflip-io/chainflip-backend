import { execSync } from "child_process";
import { getBtcBalance } from "./get_btc_balance";
import { getEthBalance } from "./get_eth_balance";
import { getDotBalance } from "./get_dot_balance";

function getBalance(ccy: string, address: string) {
    var amount = 0;
    if (ccy == "btc") {
        amount = getBtcBalance(address);
    }
    // } else if (ccy == "eth") {
    //     console.log("Getting eth Balance")
    //     amount = getEthBalance(address);
    // } else if (ccy == "dot") {
    //     amount = getDotBalance(address);
    // } else if (ccy == "usdc") {
    //     amount = getUsdcBalance(address);
    // }
}

let ccy = process.argv[2];
let address = process.argv[3];
address = address.trim();
getBalance(ccy, address)