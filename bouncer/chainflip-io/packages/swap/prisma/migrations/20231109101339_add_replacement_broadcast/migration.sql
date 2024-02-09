/*
  Warnings:

  - A unique constraint covering the columns `[replacedById]` on the table `Broadcast` will be added. If there are existing duplicate values, this will fail.

*/
-- AlterTable
ALTER TABLE "public"."Broadcast" ADD COLUMN     "replacedById" BIGINT;

-- CreateIndex
CREATE UNIQUE INDEX "Broadcast_replacedById_key" ON "public"."Broadcast"("replacedById");

-- AddForeignKey
ALTER TABLE "public"."Broadcast" ADD CONSTRAINT "Broadcast_replacedById_fkey" FOREIGN KEY ("replacedById") REFERENCES "public"."Broadcast"("id") ON DELETE SET NULL ON UPDATE CASCADE;
