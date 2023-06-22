import { performSwap } from "../shared/perform_swap";

async function main() {
    let SRC_CCY = process.argv[2];
    let DST_CCY = process.argv[3];
    let ADDRESS = process.argv[4];
    await performSwap(SRC_CCY, DST_CCY, ADDRESS);
}

main();