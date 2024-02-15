import { setupDatumViews } from "../views/setupDatumViews";
import { connectDb } from "../auth/connectDb";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../utils/parseIfNeeded";
import { MainDatumArgs } from "../input/mainArgs";

export const setupArgs = new ArgumentParser({
  add_help: false,
});
setupArgs.add_argument("project-dir", {
  help: "where to look for additional DatumViews to setup in the database",
  type: "string",
  dest: "projectDir",
});

export const setupCmdArgs = new ArgumentParser({
  description: "setup the database for use with datum",
  parents: [setupArgs],
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
