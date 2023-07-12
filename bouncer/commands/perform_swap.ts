import { performSwap } from "../shared/perform_swap";
import { Asset } from "@chainflip-io/cli/.";

async function main() {
    const SRC_CCY = process.argv[2].toUpperCase() as Asset;
    const DST_CCY = process.argv[3].toUpperCase() as Asset;
    const ADDRESS = process.argv[4];
    await performSwap(SRC_CCY, DST_CCY, ADDRESS);
    process.exit(0);
}

main();