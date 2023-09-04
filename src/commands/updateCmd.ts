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
import { QuickIdArg, quickIdArg } from "../input/quickIdArg";
import { timeYargs } from "../input/timeArgs";
import { MainDatumArgs } from "../input/mainYargs";

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
  return timeYargs(dataYargs(quickIdArg(yargs))).options({
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
  args: UpdateCmdArgs
): Promise<EitherDocument[]> {
  const db = connectDb(args);

  // process quickIds like the first required argument so that data changes can be specified beforehand in the command
  // for easier aliasing
  if (typeof args.quickId === "string") {
    args.required ??= [];
    args.required = ["__quickId"].concat(args.required);
    args.data ??= [];
    args.data.unshift(args.quickId);
  }
  const {
    __quickId,
    ...payload
  }: DatumData<{ __quickId?: string | string[] }> = handleDataArgs(args);

  const ids = await quickIds(db, __quickId ?? args.quickId);

  const updateStrategy = args.strategy ?? "preferNew";

  const updatedDocs = await Promise.all(
    ids.map((id) =>
      updateDoc({
        db,
        id,
        payload,
        updateStrategy,
        outputArgs: args,
      })
    )
  );

  return updatedDocs;
}
