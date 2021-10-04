import { BaseDatumArgs } from "../input/baseYargs";
import { DataInputArgs } from "../input/dataArgs";
import { UpdateStrategyNames } from "../documentControl/combineData";
import { EitherDocument } from "../documentControl/DatumDocument";
import inferType from "../utils/inferType";
import { BaseDataError } from "../errors";
import { parseData } from "../parseData";
import connectDb from "../auth/connectDb";
import { Show } from "../output";
import updateDoc from "../documentControl/updateDoc";
import quickId from "../ids/quickId";

export const command = ["update <quickId> [data..]", "merge <quickId> [data..]"];
export const desc = "Update the data in an existing document";

export type UpdateCmdArgs = BaseDatumArgs & DataInputArgs & {
  quickId: string;
  strategy?: UpdateStrategyNames;
}

export async function updateCmd(args: UpdateCmdArgs): Promise<EitherDocument> {
  const db = connectDb(args);

  const id = await quickId(db, args.quickId);
  const payload = parseData(args);
  const updateStrategy = args.strategy ?? "preferNew";
  const show: Show = args.showAll ? Show.All : args.show ?? Show.None;

  const doc = await updateDoc({db, id, payload, updateStrategy, show});

  return doc;
}