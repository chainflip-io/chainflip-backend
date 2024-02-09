-- CreateTable
CREATE TABLE "private"."DepositChannel" (
    "id" BIGSERIAL NOT NULL,
    "channelId" BIGINT NOT NULL,
    "srcChain" "public"."Chain" NOT NULL,
    "issuedBlock" INTEGER NOT NULL,
    "depositAddress" TEXT NOT NULL,
    "isSwapping" BOOLEAN NOT NULL,

    CONSTRAINT "DepositChannel_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE INDEX "DepositChannel_depositAddress_idx" ON "private"."DepositChannel"("depositAddress");

-- CreateIndex
CREATE UNIQUE INDEX "DepositChannel_issuedBlock_srcChain_channelId_key" ON "private"."DepositChannel"("issuedBlock", "srcChain", "channelId");

-- backfill existing information
INSERT INTO "private"."DepositChannel" ("channelId", "srcChain", "issuedBlock", "depositAddress", "isSwapping")
SELECT "channelId", "srcChain", "issuedBlock", "depositAddress", TRUE
FROM "public"."SwapDepositChannel";
