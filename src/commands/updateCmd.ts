import { BaseDatumArgs } from "../input/baseYargs";
import { DataArgs, dataYargs, handleDataArgs } from "../input/dataArgs";
import {
  updateStrategies,
  UpdateStrategyNames,
} from "../documentControl/combineData";
import { EitherDocument } from "../documentControl/DatumDocument";
import connectDb from "../auth/connectDb";
import { Show } from "../output";
import updateDoc from "../documentControl/updateDoc";
import quickId from "../ids/quickId";
import { Argv } from "yargs";
import { quickIdArg } from "../input/quickIdArg";
import { timeYargs } from "../input/timeArgs";

export const command = [
  "update <quickId> [data..]",
  "merge <quickId> [data..]",
];
export const desc = "Update the data in an existing document";

export type UpdateCmdArgs = BaseDatumArgs &
  DataArgs & {
    quickId: string;
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

export async function updateCmd(args: UpdateCmdArgs): Promise<EitherDocument> {
  const db = connectDb(args);

  const id = await quickId(db, args.quickId);
  const payload = handleDataArgs(args);
  const updateStrategy = args.strategy ?? "preferNew";
  const show: Show = args.showAll ? Show.All : args.show ?? Show.None;

  const doc = await updateDoc({ db, id, payload, updateStrategy, show });

  return doc;
}
