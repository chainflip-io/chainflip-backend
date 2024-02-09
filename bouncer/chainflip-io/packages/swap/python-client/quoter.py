from typing import Any
from abc import ABC, abstractmethod
from cryptography.hazmat.primitives import serialization
from dataclasses import dataclass
import base64, socketio, time
from typing import Optional, Dict, Tuple


@dataclass
class Quote:
    id: str
    source_asset: str
    destination_asset: str
    deposit_amount: str

    def __init__(self, json: Dict[str, Any]):
        self.id = json["id"]
        self.source_asset = json["source_asset"]
        self.destination_asset = json["destination_asset"]
        self.deposit_amount = json["deposit_amount"]


class Quoter(ABC):
    connected = False
    sio: Optional[socketio.AsyncClient] = None

    @abstractmethod
    async def on_quote_request(self, quote: Quote) -> Tuple[str, str]:
        """
        :param quote: Quote object
        :return: (intermediate_amount, egress_amount)
        """
        pass

    def on_connect(self):
        pass

    async def send_quote(self, response: Dict[str, str]):
        if self.connected and self.sio is not None:
            await self.sio.emit("quote_response", response)

    async def connect(
        self,
        market_maker_id: str,
        private_key_bytes: bytes,
        url: str,
        password: Optional[str] = None,
        wait_timeout: int = 1,
    ):
        self.sio = socketio.AsyncClient()

        @self.sio.event
        async def connect():
            self.connected = True
            self.on_connect()

        @self.sio.event
        async def quote_request(data: Dict[str, Any]):
            quote = Quote(data)
            (intermediate_amount, egress_amount) = await self.on_quote_request(quote)
            await self.send_quote(
                {
                    "id": quote.id,
                    "intermediate_amount": intermediate_amount,
                    "egress_amount": egress_amount,
                }
            )

        timestamp = round(time.time() * 1000)
        signature = serialization.load_pem_private_key(
            private_key_bytes, password=password
        ).sign(
            b"%b%b" % (bytes(market_maker_id, "utf-8"), bytes(str(timestamp), "utf-8"))
        )

        await self.sio.connect(
            url,
            auth={
                "client_version": "1",
                "timestamp": timestamp,
                "market_maker_id": market_maker_id,
                "signature": base64.b64encode(signature).decode("utf-8"),
            },
            wait_timeout=wait_timeout,
        )
        await self.sio.wait()

    async def disconnect(self):
        if self.connected and self.sio is not None:
            await self.sio.disconnect()
            self.connected = False
