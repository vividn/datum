import { Argv } from "yargs";
import { handleTimeArgs } from "../input/timeArgs";
import { addArgs, addCmd } from "./addCmd";
import { parseBaseData } from "../input/dataArgs";
import { EitherDocument } from "../documentControl/DatumDocument";
import { StartCmdArgs } from "./startCmd";

export const command = "end <field> [data..]";
export const desc = "add an end document";

export function builder(yargs: Argv): Argv {
  return addArgs(yargs).options({}).positional("field", {
    describe: "what is being tracked",
    type: "string",
    nargs: 1,
  });
}

export type EndCmdArgs = StartCmdArgs;

export async function endCmd(args: StartCmdArgs): Promise<EitherDocument> {
  const { timeStr: occurTime, utcOffset } = handleTimeArgs(args);
  const parsedData = parseBaseData(args.baseData);
  parsedData.state = false;
  if (occurTime !== undefined) {
    parsedData.occurTime = occurTime;
    parsedData.occurUtcOffset = utcOffset;
  }
  return await addCmd({ ...args, baseData: parsedData });
}
