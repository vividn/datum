import { dataArgs, DataArgs, handleDataArgs } from "../input/dataArgs.js";
import {
  updateStrategies,
  UpdateStrategyNames,
} from "../documentControl/combineData.js";
import { DatumData, EitherDocument } from "../documentControl/DatumDocument.js";
import { connectDb } from "../auth/connectDb.js";
import { updateDoc } from "../documentControl/updateDoc.js";
import { QuickIdArgs, quickIdArgs } from "../input/quickIdArg.js";
import { flexiblePositional } from "../input/flexiblePositional.js";
import { updateLastDocsRef } from "../documentControl/lastDocs.js";
import { dbArgs } from "../input/dbArgs.js";
import { outputArgs } from "../input/outputArgs.js";
import { ArgumentParser } from "argparse";
import { MainDatumArgs } from "../input/mainArgs.js";
import { parseIfNeeded } from "../utils/parseIfNeeded.js";
import { quickId, _LAST_WITH_PROTECTION } from "../ids/quickId.js";
import { JsonType } from "../utils/utilityTypes.js";

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
    " Defaults to 'update' strategy for update command." +
    " Defaults to 'merge' for the merge command",
  prog: "datum update/merge",
  usage: `%(prog)s <quickId> [data..]
  %(prog)s --strategy <strategy> <quickId> [data..]`,
  parents: [updateArgs],
});

export type UpdateCmdArgs = MainDatumArgs &
  DataArgs &
  QuickIdArgs & {
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
  flexiblePositional(args, "quickId", `__quickId=${_LAST_WITH_PROTECTION}`);
  const { __quickId, ...payload }: DatumData<{ __quickId?: JsonType }> =
    handleDataArgs(args);

  const ids = await quickId(__quickId ?? _LAST_WITH_PROTECTION, args);

  // update now in case the updateDoc fails due to conflict
  await updateLastDocsRef(db, ids);

  const updateStrategy = args.strategy ?? "update";

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
