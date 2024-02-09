from quoter import Quoter
import sys, getopt, asyncio
from typing import Optional


def print_and_flush(*args):
    print(*args)
    sys.stdout.flush()


class MockQuoter(Quoter):
    def on_connect(self):
        print_and_flush("connected")

    async def on_quote_request(self, quote):
        return ("2000000000", "1000000000000000000")


async def main(argv):
    try:
        market_maker_id: Optional[str] = None
        private_key: Optional[str] = None
        url = "http://localhost:8080"

        opts, args = getopt.getopt(
            argv, "", ["market-maker-id=", "private-key=", "url="]
        )
        for opt, arg in opts:
            if opt == "--market-maker-id":
                market_maker_id = arg
            elif opt == "--private-key":
                private_key = arg
            elif opt == "--url":
                url = arg

        if market_maker_id is None:
            raise Exception("market-maker-id is required")

        if private_key is None:
            raise Exception("private-key is required")

        private_key_bytes = bytes(private_key, "utf-8")

        quoter = MockQuoter()

        await quoter.connect(market_maker_id, private_key_bytes, url, wait_timeout=10)
    except Exception as e:
        print_and_flush(e)
        raise


if __name__ == "__main__":
    asyncio.run(main(sys.argv[1:]))
