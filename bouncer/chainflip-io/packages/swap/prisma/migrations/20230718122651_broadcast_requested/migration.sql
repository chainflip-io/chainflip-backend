/*
  Warnings:

  - You are about to drop the column `timestamp` on the `Egress` table. All the data in the column will be lost.
  - You are about to drop the column `egressCompletedAt` on the `Swap` table. All the data in the column will be lost.
  - You are about to drop the column `egressCompletedBlockIndex` on the `Swap` table. All the data in the column will be lost.
  - Added the required column `requestedAt` to the `Broadcast` table without a default value. This is not possible if the table is not empty.
  - Added the required column `requestedBlockIndex` to the `Broadcast` table without a default value. This is not possible if the table is not empty.
  - Added the required column `scheduledAt` to the `Egress` table without a default value. This is not possible if the table is not empty.
  - Added the required column `scheduledBlockIndex` to the `Egress` table without a default value. This is not possible if the table is not empty.

*/
-- AlterTable
ALTER TABLE "public"."Broadcast" ADD COLUMN     "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
ADD COLUMN     "requestedAt" TIMESTAMP(3) NOT NULL,
ADD COLUMN     "requestedBlockIndex" TEXT NOT NULL,
ADD COLUMN     "succeededAt" TIMESTAMP(3),
ADD COLUMN     "succeededBlockIndex" TEXT,
ADD COLUMN     "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP;

-- AlterTable
ALTER TABLE "public"."Egress" RENAME COLUMN "timestamp" TO "scheduledAt";
ALTER TABLE "public"."Egress" ADD COLUMN     "scheduledBlockIndex" TEXT NOT NULL,
ADD COLUMN     "updatedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP;

-- AlterTable
ALTER TABLE "public"."Swap" DROP COLUMN "egressCompletedAt",
DROP COLUMN "egressCompletedBlockIndex";
