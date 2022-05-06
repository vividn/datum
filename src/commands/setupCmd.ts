import { Argv } from "yargs";
import { BaseDatumArgs } from "../input/baseYargs";
import { setupDatumViews } from "../views/setupDatumViews";
import { connectDb, connectNano } from "../auth/connectDb";
import { Show } from "../output/output";

export const command = "setup";
export const desc = "setup the database for use with datum";

export type SetupCmdArgs = BaseDatumArgs & {
  projectDir?: string;
};

export function builder(yargs: Argv): Argv {
  return yargs.options({
    "project-dir": {
      describe:
        "where to look for additional DatumViews to setup in the database",
      type: "string",
    },
  });
}

export async function setupCmd(args: SetupCmdArgs): Promise<void> {
  const db = await connectDb(args);
  const show: Show = args.showAll ? Show.All : args.show ?? Show.None;
  await setupDatumViews({ db, show: show, projectDir: args.projectDir });
}
