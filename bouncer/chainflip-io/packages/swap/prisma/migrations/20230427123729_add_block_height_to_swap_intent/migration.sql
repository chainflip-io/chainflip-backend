-- AlterTable
ALTER TABLE "public"."SwapIntent" ADD COLUMN     "blockHeight" INTEGER;

-- CreateTable
CREATE TABLE "public"."SwapIntentBlock" (
    "id" BIGSERIAL NOT NULL,
    "ingressAddress" TEXT NOT NULL,
    "blockHeight" INTEGER NOT NULL,

    CONSTRAINT "SwapIntentBlock_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE INDEX "SwapIntentBlock_blockHeight_idx" ON "public"."SwapIntentBlock"("blockHeight");
