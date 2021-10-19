import { DateTime, Duration } from "luxon";

type ParseDurationStrType = {
  durationStr: string;
};

export const parseDurationStr = function ({
  durationStr,
}: ParseDurationStrType): Duration {
  return Duration.fromObject({minutes: 1});
};
