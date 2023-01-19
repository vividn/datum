import { Argv } from "yargs";
import { setupDatumViews } from "../views/setupDatumViews";
import { connectDb } from "../auth/connectDb";
import { MainDatumArgs } from "../input/mainYargs";

export const command = "setup";
export const desc = "setup the database for use with datum";

export type SetupCmdArgs = MainDatumArgs & {
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
  args.createDb ??= true;
  const db = await connectDb(args);
  await setupDatumViews({ db, outputArgs: args, projectDir: args.projectDir });
}
