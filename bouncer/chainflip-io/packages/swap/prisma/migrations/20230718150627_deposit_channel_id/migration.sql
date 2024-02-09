/*
  Warnings:

  - You are about to drop the column `uuid` on the `SwapDepositChannel` table. All the data in the column will be lost.
  - A unique constraint covering the columns `[issuedBlock,srcChain,channelId]` on the table `SwapDepositChannel` will be added. If there are existing duplicate values, this will fail.
  - Added the required column `channelId` to the `SwapDepositChannel` table without a default value. This is not possible if the table is not empty.
  - Added the required column `srcChain` to the `SwapDepositChannel` table without a default value. This is not possible if the table is not empty.

*/
-- DropIndex
DROP INDEX "public"."SwapDepositChannel_uuid_key";

-- AlterTable
ALTER TABLE "public"."SwapDepositChannel" DROP COLUMN "uuid",
ADD COLUMN     "channelId" BIGINT NOT NULL,
ADD COLUMN     "srcChain" "public"."Chain" NOT NULL;

-- CreateIndex
CREATE UNIQUE INDEX "SwapDepositChannel_issuedBlock_srcChain_channelId_key" ON "public"."SwapDepositChannel"("issuedBlock", "srcChain", "channelId");
