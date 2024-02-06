import { DataArgs, dataYargs, handleDataArgs } from "../input/dataArgs";
import {
  updateStrategies,
  UpdateStrategyNames,
} from "../documentControl/combineData";
import { DatumData, EitherDocument } from "../documentControl/DatumDocument";
import { connectDb } from "../auth/connectDb";
import { updateDoc } from "../documentControl/updateDoc";
import { quickIds } from "../ids/quickId";
import { Argv } from "yargs";
import { QuickIdArg, quickIdArgs } from "../input/quickIdArg";
import { timeYargs } from "../input/timeArgs";
import { MainDatumArgs } from "../input/MainArgs";
import { flexiblePositional } from "../input/flexiblePositional";
import { updateLastDocsRef } from "../documentControl/lastDocs";

export const command = [
  "update <quickId> [data..]",
  "merge <quickId> [data..]",
];
export const desc = "Update the data in an existing document";

export type UpdateCmdArgs = MainDatumArgs &
  DataArgs &
  QuickIdArg & {
    strategy?: UpdateStrategyNames;
  };

export function builder(yargs: Argv): Argv {
  return timeYargs(dataYargs(quickIdArgs(yargs))).options({
    strategy: {
      describe:
        "which update strategy to use when modifying the doc." +
        " Defaults to 'preferNew' for update command." +
        " Defaults to 'merge' for merge command.",
      type: "string",
      alias: "X",
      choices: Object.keys(updateStrategies),
    },
  });
}

export async function updateCmd(
  args: UpdateCmdArgs,
): Promise<EitherDocument[]> {
  const db = connectDb(args);

  // process quickIds like the first required argument so that data changes can be specified beforehand in the command
  // for easier aliasing
  flexiblePositional(args, "quickId", "required", "__quickId");
  const {
    __quickId,
    ...payload
  }: DatumData<{ __quickId?: string | string[] }> = handleDataArgs(args);

  const ids = await quickIds(db, __quickId ?? args.quickId);

  // update now in case the updateDoc fails due to conflict
  await updateLastDocsRef(db, ids);

  const updateStrategy = args.strategy ?? "preferNew";

  const updatedDocs = await Promise.all(
    ids.map((id) =>
      updateDoc({
        db,
        id,
        payload,
        updateStrategy,
        outputArgs: args,
      }),
    ),
  );

  const newIds = updatedDocs.map((doc) => doc._id);
  if (newIds !== ids) {
    // TODO: if changing lastDocs to history may need to change this to overwrite first update
    await updateLastDocsRef(db, newIds);
  }

  return updatedDocs;
}
