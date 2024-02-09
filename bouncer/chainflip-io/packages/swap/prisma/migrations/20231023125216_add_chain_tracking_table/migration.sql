-- CreateTable
CREATE TABLE "public"."ChainTracking" (
    "id" SERIAL NOT NULL,
    "chain" "public"."Chain" NOT NULL,
    "height" BIGINT NOT NULL DEFAULT 0,
    "updatedAt" TIMESTAMPTZ(3) NOT NULL,

    CONSTRAINT "ChainTracking_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE UNIQUE INDEX "ChainTracking_chain_key" ON "public"."ChainTracking"("chain");
