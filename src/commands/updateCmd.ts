import { dataArgs, DataArgs, handleDataArgs } from "../input/dataArgs";
import {
  updateStrategies,
  UpdateStrategyNames,
} from "../documentControl/combineData";
import { DatumData, EitherDocument } from "../documentControl/DatumDocument";
import { connectDb } from "../auth/connectDb";
import { updateDoc } from "../documentControl/updateDoc";
import { quickIds } from "../ids/quickId";
import { QuickIdArg, quickIdArgs } from "../input/quickIdArg";
import { flexiblePositional } from "../input/flexiblePositional";
import { updateLastDocsRef } from "../documentControl/lastDocs";
import { dbArgs } from "../input/dbArgs";
import { outputArgs } from "../input/outputArgs";
import { ArgumentParser } from "argparse";
import { MainDatumArgs } from "../input/mainArgs";
import { parseIfNeeded } from "../utils/parseIfNeeded";

export const updateArgs = new ArgumentParser({
  add_help: false,
  parents: [quickIdArgs, dataArgs, dbArgs, outputArgs],
});
updateArgs.add_argument("--strategy", "-X", {
  help: "which update strategy to use when modifying the doc",
  choices: Object.keys(updateStrategies),
});

export const updateCmdArgs = new ArgumentParser({
  description:
    "Update the data in an existing document." +
    " Defaults to 'preferNew' strategy for update command." +
    " Defaults to 'merge' for the merge command",
  prog: "dtmUpdate",
  usage: `%(prog)s <quickId> [data..]
  %(prog)s --strategy <strategy> <quickId> [data..]`,
  parents: [updateArgs],
});

export type UpdateCmdArgs = MainDatumArgs &
  DataArgs &
  QuickIdArg & {
    strategy?: UpdateStrategyNames;
  };

export async function updateCmd(
  argsOrCli: UpdateCmdArgs | string | string[],
  preparsed?: Partial<UpdateCmdArgs>,
): Promise<EitherDocument[]> {
  const args = parseIfNeeded(updateCmdArgs, argsOrCli, preparsed);
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
