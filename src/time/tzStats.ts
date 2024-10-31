import { isoDate, isoDatetime } from "./timeUtils";

type TzStatsArgs = {
  db: PouchDB.Database;
} & (
  | {
      date: isoDate;
    }
  | { startTime: isoDatetime; endTime: isoDatetime }
);

type TzStats = {
  utcDomain: [isoDatetime, isoDatetime];
  localDomain: [number, number];
  tzBlocks: {
    localDomain: [number, number];
    tracks: { tz: number; utcDomain: [isoDatetime, isoDatetime] }[];
  }[];
};
export async function tzStats(args: TzStatsArgs): Promise<TzStats> {
  return;
};
