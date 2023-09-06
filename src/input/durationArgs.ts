import { Argv } from "yargs";

export type DurationArgs = {
  duration?: string | number;
  moment?: boolean;
};
export function durationArgs(yargs: Argv): Argv {
  return yargs
    .options({
      moment: {
        alias: "m",
        describe:
          "don't interpret the first argument after field as a duration",
        nargs: 0,
      },
    })
    .positional("duration", {
      describe:
        "how long the event lasted, default units is minutes, but other forms can be used." +
        " 5 = 5 minutes, 5h = 5 hours, 5:35:35 = 5 hours 35 minutes and 35 seconds, etc." +
        'a single period "." or empty string "" can be used to indicate no duration (for example to allow' +
        " entering in of other data without specifying a duration)",
      nargs: 1,
    });
  //TODO: Include more help here about placement of duration between required values and optional values
}