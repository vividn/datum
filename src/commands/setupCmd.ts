import { Argv } from "yargs";
import { BaseDatumArgs } from "../input/baseYargs";
import setupDatumViews from "../views/setupDatumViews";
import connectDb from "../auth/connectDb";
import { Show } from "../output";

export const command = "setup";
export const desc = "setup the database for use with datum"

export type SetupCmdArgs = BaseDatumArgs;

export function builder(yargs: Argv): Argv {
  return yargs;
}

export async function setupCmd(args: SetupCmdArgs): Promise<void> {
  const db = connectDb(args);
  const show: Show = args.showAll ? Show.All : args.show ?? Show.None;
  await setupDatumViews({db, show: args.show});
}

export default setupCmd;