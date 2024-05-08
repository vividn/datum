import { ArgumentParser } from "argparse";

export type DurationArgs = {
  duration?: string | number;
  moment?: boolean;
};
const durationArgs = new ArgumentParser({
  add_help: false,
});
const durationGroup = durationArgs.add_argument_group({
  title: "Duration",
  description: "Options for specifying the duration of an event",
});

durationGroup.add_argument("duration", {
  help:
    "how long the event lasted, default units is minutes, but other forms can be used." +
    " 5 = 5 minutes, 5h = 5 hours, 5:35:35 = 5 hours 35 minutes and 35 seconds, etc." +
    'a single period "." or empty string "" can be used to indicate no duration (for example to allow' +
    " entering in of other data without specifying a duration)",
  nargs: "?",
});
durationGroup.add_argument("-m", "--moment", {
  help: "data with no indication of duration, just occurrence. Removes positional duration argument. Sets dur to be null. Essentially equivalent to an occur cmd",
  action: "store_true",
});

export { durationArgs };
