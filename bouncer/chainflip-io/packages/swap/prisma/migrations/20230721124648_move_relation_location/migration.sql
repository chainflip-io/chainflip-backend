/*
  Warnings:

  - You are about to drop the column `swapId` on the `Egress` table. All the data in the column will be lost.

*/
-- DropForeignKey
ALTER TABLE "public"."Egress" DROP CONSTRAINT "Egress_swapId_fkey";

-- DropIndex
DROP INDEX "public"."Egress_swapId_key";

-- AlterTable
ALTER TABLE "public"."Egress" DROP COLUMN "swapId";

-- AlterTable
ALTER TABLE "public"."Swap" ADD COLUMN     "egressId" BIGINT;

-- AddForeignKey
ALTER TABLE "public"."Swap" ADD CONSTRAINT "Swap_egressId_fkey" FOREIGN KEY ("egressId") REFERENCES "public"."Egress"("id") ON DELETE SET NULL ON UPDATE CASCADE;
