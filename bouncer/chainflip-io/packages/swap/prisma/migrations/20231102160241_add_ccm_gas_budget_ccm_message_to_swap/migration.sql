-- AlterTable
ALTER TABLE "public"."Swap" ADD COLUMN     "ccmDepositReceivedBlockIndex" TEXT,
ADD COLUMN     "ccmGasBudget" DECIMAL(30,0),
ADD COLUMN     "ccmMessage" TEXT;
