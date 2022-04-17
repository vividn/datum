import { Argv } from "yargs";
import { BaseDatumArgs } from "../input/baseYargs";
import { setupDatumViews } from "../views/setupDatumViews";
import { connectDb } from "../auth/connectDb";
import { Show } from "../output/output";
import { defaults } from "../input/defaults";

export const command = "setup";
export const desc = "setup the database for use with datum";

export type SetupCmdArgs = BaseDatumArgs & {
  projectDir?: string;
};

export function builder(yargs: Argv): Argv {
  return yargs.options({
    "project-dir": {
      describe: "where to look for additional DatumViews to setup in the database",
      type: "string",
    }
  });
}

export async function setupCmd(args: SetupCmdArgs): Promise<void> {
  const db = connectDb(args);
  const show: Show = args.showAll ? Show.All : args.show ?? Show.None;
  const projectDir = args.projectDir ?? defaults.projectDir;
  await setupDatumViews({ db, show: show, projectDir });
}
