-- AlterTable
ALTER TABLE "public"."Broadcast" ADD COLUMN     "abortedAt" TIMESTAMP(3),
ADD COLUMN     "abortedBlockIndex" TEXT;
