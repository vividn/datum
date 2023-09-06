import { Argv } from "yargs";
import { occurArgs, OccurCmdArgs } from "./occurCmd";
import { EitherDocument } from "../documentControl/DatumDocument";
import { parseBaseData } from "../input/dataArgs";
import { handleTimeArgs } from "../input/timeArgs";
import { getActiveState } from "../state/getActiveState";
import { connectDb } from "../auth/connectDb";
import { DateTime } from "luxon";
import { inferType } from "../utils/inferType";
import { addCmd } from "./addCmd";
import { flexiblePositional } from "../input/flexiblePositional";

export const command = [
  "switch <field> <state> [duration] [data..]",
  "switch --moment <field> <state> [data..]",
];
export const desc = "switch states of a given field";

export function builder(yargs: Argv): Argv {
  return occurArgs(yargs)
    .positional("state", {
      describe: "the state to switch to",
      type: "string",
      nargs: 1,
    })
    .options({
      "last-state": {
        describe: "manually specify the last state being transitioned out of",
        nargs: 1,
        // TODO: add alias l here after switching lenient to strict
      },
    });
}

export type SwitchCmdArgs = OccurCmdArgs & {
  state: string | boolean | null;
  lastState?: string | boolean | null;
};

export async function switchCmd(args: SwitchCmdArgs): Promise<EitherDocument> {
  flexiblePositional(args, "duration", "optional", "dur");
  flexiblePositional(args, "state", "required");
  flexiblePositional(args, "field", "required");
  const parsedData = parseBaseData(args.baseData);

  const { timeStr: occurTime, utcOffset } = handleTimeArgs(args);
  if (occurTime !== undefined) {
    parsedData.occurTime = occurTime;
    parsedData.occurUtcOffset = utcOffset;
  }
  parsedData.state =
    typeof args.state === "string" ? inferType(args.state) : args.state;
  if (args.lastState !== undefined) {
    parsedData.lastState =
      args.lastState === "string" ? inferType(args.lastState) : args.lastState;
  } else if (occurTime !== undefined) {
    const db = connectDb(args);
    parsedData.lastState = await getActiveState(
      db,
      args.field,
      DateTime.fromISO(occurTime)
    );
  }
  return await addCmd({ ...args, baseData: parsedData });
}
