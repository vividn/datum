import { Argv } from "yargs";
import { handleTimeArgs, TimeArgs } from "../input/timeArgs";
import { addArgs, addCmd, AddCmdArgs } from "./addCmd";
import { parseBaseData } from "../input/dataArgs";
import { EitherDocument } from "../documentControl/DatumDocument";

export const command = "start <field> [data..]";
export const desc = "add a start document";

export function builder(yargs: Argv): Argv {
  return addArgs(yargs).options({}).positional("field", {
    describe: "what is being tracked",
    type: "string",
    nargs: 1,
  });
}

export type OccurCmdArgs = AddCmdArgs &
  TimeArgs & {
    duration?: string;
  };

export async function startCmd(args: OccurCmdArgs): Promise<EitherDocument> {
  const { timeStr: occurTime, utcOffset } = handleTimeArgs(args);
  const parsedData = parseBaseData(args.baseData);
  parsedData.state = true;
  if (occurTime !== undefined) {
    parsedData.occurTime = occurTime;
    parsedData.occurUtcOffset = utcOffset;
  }
  return await addCmd({ ...args, baseData: parsedData });
}
