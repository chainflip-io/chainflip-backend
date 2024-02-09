-- CreateTable
CREATE TABLE "public"."Pool" (
    "id" SERIAL NOT NULL,
    "baseAsset" "public"."Asset" NOT NULL,
    "pairAsset" "public"."Asset" NOT NULL,
    "feeHundredthPips" INTEGER NOT NULL,

    CONSTRAINT "Pool_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE UNIQUE INDEX "Pool_baseAsset_pairAsset_key" ON "public"."Pool"("baseAsset", "pairAsset");
