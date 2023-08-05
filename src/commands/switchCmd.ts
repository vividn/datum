import { Argv } from "yargs";
import { addArgs, addCmd } from "./addCmd";
import { occurArgs, OccurCmdArgs } from "./occurCmd";
import { EitherDocument } from "../documentControl/DatumDocument";

export const command = [
  "switch <field> <state> [duration] [data..]",
  "occur --moment <field> <state> [data..]",
];
export const desc = "switch states of a given field";

// export function builder(yargs: Argv): Argv {
//   return addArgs(yargs)
//     .options({
//       moment: {
//         alias: "m",
//         describe:
//           "don't interpret the first argument after field as a duration",
//         nargs: 0,
//       },
//     })
//     .positional("field", {
//       describe: "what is being tracked",
//       type: "string",
//       nargs: 1,
//     })
//     .positional("state", {
//       describe: "the state to switch to",
//       type: "string",
//       nargs: 1,
//     })
//     .positional("duration", {
//       describe:
//         "how long the event lasted, default units is minutes, but other forms can be used." +
//         " 5 = 5 minutes, 5h = 5 hours, 5:35:35 = 5 hours 35 minutes and 35 seconds, etc." +
//         'a single period "." or empty string "" can be used to indicate no duration (for example to allow' +
//         " entering in of other data without specifying a duration)",
//       type: "string",
//       nargs: 1,
//     });
// }
export function builder(yargs: Argv): Argv {
  return occurArgs(yargs).positional("state", {
    describe: "the state to switch to",
    type: "string",
    nargs: 1,
  });
}

export type SwitchCmdArgs = OccurCmdArgs & {
  state: string;
};

export function switchCmd(args: SwitchCmdArgs): Promise<EitherDocument> {
  console.log({ args });
  return addCmd(args);
}