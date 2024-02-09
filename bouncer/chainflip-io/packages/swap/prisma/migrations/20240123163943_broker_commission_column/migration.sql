-- AlterTable
ALTER TABLE "public"."SwapDepositChannel" ADD COLUMN     "brokerCommissionBps" INTEGER NOT NULL DEFAULT 0;
ALTER TABLE "public"."SwapDepositChannel" ALTER COLUMN "brokerCommissionBps" DROP DEFAULT;
