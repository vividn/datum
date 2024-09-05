import { ArgumentParser } from "argparse";
import { FieldArgs } from "../input/fieldArgs";
import { outputArgs } from "../input/outputArgs";
import { dbArgs } from "../input/dbArgs";
import { MainDatumArgs } from "../input/mainArgs";
import { checkState } from "../state/checkState";
import { parseIfNeeded } from "../utils/parseIfNeeded";
import { connectDb } from "../auth/connectDb";

export const checkArgs = new ArgumentParser({
  add_help: false,
  parents: [dbArgs, outputArgs],
});
checkArgs.add_argument("field", {
  help: "the data to check. Defaults to check all fields",
  nargs: "?",
});

export const checkCmdArgs = new ArgumentParser({
  description: "Check for problems in the data",
  prog: "datum check",
  usage: `%(prog)s [field]`,
  parents: [checkArgs],
});

export type CheckCmdArgs = FieldArgs & MainDatumArgs;

export async function checkCmd(
  argsOrCli: CheckCmdArgs | string | string[],
  preparsed?: Partial<CheckCmdArgs>,
): Promise<boolean> {
  const args = parseIfNeeded(checkCmdArgs, argsOrCli, preparsed);
  const db = connectDb(args);
  const fields = args.field?.split(",") ?? []; // TODO: get all fields
  await Promise.all(fields.map((field) => checkState({ db, field })));
  return true;
}
