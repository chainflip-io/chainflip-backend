-- AlterTable
ALTER TABLE "public"."Swap" ADD COLUMN     "egressAmount" DECIMAL(30,0),
ADD COLUMN     "intermediateAmount" DECIMAL(30,0);
