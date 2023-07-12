import { performSwap } from "../shared/perform_swap";

async function main() {
    const SRC_CCY = process.argv[2];
    const DST_CCY = process.argv[3];
    const ADDRESS = process.argv[4];
    await performSwap(SRC_CCY, DST_CCY, ADDRESS);
    process.exit(0);
}

main();