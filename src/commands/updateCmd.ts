import { BaseDatumArgs } from "../input/baseYargs";
import { DataInputArgs } from "../input/dataArgs";
import { UpdateStrategyNames } from "../documentControl/combineData";
import { EitherDocument } from "../documentControl/DatumDocument";
import inferType from "../utils/inferType";
import { BaseDataError } from "../errors";

export const command = ["update <quickId> [data..]", "merge <quickId> [data..]"];
export const desc = "Update the data in an existing document";

export type UpdateCmdArgs = BaseDatumArgs & DataInputArgs & {
  quickId: string;
  strategy?: UpdateStrategyNames;
}

export async function updateCmd(args: UpdateCmdArgs): Promise<EitherDocument> {
  const strategy = args.strategy ?? "preferNew";

  const baseData = args.baseData ? inferType(args.baseData) : {};
  if (typeof baseData !== "object" || baseData === null) {
    throw new BaseDataError("base data not a valid object");
  }

  return {_id: "", _rev: ""};
}