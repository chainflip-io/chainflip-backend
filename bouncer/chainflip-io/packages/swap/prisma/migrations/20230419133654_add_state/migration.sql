-- CreateTable
CREATE TABLE "State" (
    "id" SERIAL NOT NULL,
    "height" INTEGER NOT NULL DEFAULT 0,

    CONSTRAINT "State_pkey" PRIMARY KEY ("id")
);
