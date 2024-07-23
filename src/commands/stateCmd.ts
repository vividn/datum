import { fieldArgs, FieldArgs } from "../input/fieldArgs";
import { TimeArgs, timeYargs } from "../input/timeArgs";
import { Argv } from "yargs";

export const command = "state [field]";
export const desc = "get the current state of a field or all fields";

export type StateCmdArgs = FieldArgs & TimeArgs;

export function builder(yargs: Argv): Argv {
  return fieldArgs(timeYargs(yargs));
}

export async function stateCmd(args: StateCmdArgs) {

}