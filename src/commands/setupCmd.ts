import { setupDatumViews } from "../views/setupDatumViews";
import { connectDb } from "../auth/connectDb";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../utils/parseIfNeeded";
import { MainDatumArgs } from "../input/mainArgs";
import { outputArgs } from "../input/outputArgs";
import { dbArgs } from "../input/dbArgs";

export const setupArgs = new ArgumentParser({
  add_help: false,
});
setupArgs.add_argument("--project-dir", {
  help: "where to look for additional DatumViews to setup in the database",
  type: "string",
  dest: "projectDir",
});

export const setupCmdArgs = new ArgumentParser({
  description: "setup the database for use with datum",
  parents: [setupArgs, dbArgs, outputArgs],
});

export type SetupCmdArgs = MainDatumArgs & {
  projectDir?: string;
};

export async function setupCmd(
  args: SetupCmdArgs | string | string[],
  preparsed?: Partial<SetupCmdArgs>,
): Promise<void> {
  args = parseIfNeeded(setupCmdArgs, args, preparsed);
  args.createDb ??= true;
  const db = connectDb(args);
  await setupDatumViews({ db, outputArgs: args, projectDir: args.projectDir });
}
