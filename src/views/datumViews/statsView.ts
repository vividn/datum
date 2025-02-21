import { DatumView } from "../DatumView";
import { DatumDocument, DatumData } from "../../documentControl/DatumDocument";
import { DateTime } from 'luxon';
import { DatumTime } from "../../../src/time/datumTime";
import { isoDuration } from "../../../src/time/timeUtils";
import { _emit } from "../../../src/views/emit";

type StatsData = DatumData & {
  occurTime?: DatumTime;
  dur?: isoDuration;
  field?: string;
};

type DocType = DatumDocument<StatsData>;
type MapKey = [string, string, number]; // [date, field, hour]
type MapValue = number; // minutes
type ReduceValue = number;

export const statsView: DatumView<DocType, MapKey, MapValue, ReduceValue> = {
  name: "stats/by_hour",
  map: function (doc: DocType) {
    if (!doc.data?.occurTime) return;

    // Handle point-in-time events
    if (!doc.data.dur) {
      const dt = DateTime.fromISO(doc.data.occurTime.utc);
      _emit([
        dt.toISODate(),
        doc.data.field || 'unknown',
        dt.hour
      ], 1);
      return;
    }

    // Handle duration events
    const start = DateTime.fromISO(doc.data.occurTime.utc);
    const durationMs = parseDuration(doc.data.dur);
    const end = start.plus({ milliseconds: durationMs });

    // Emit minutes for each hour the event spans
    let current = start;
    while (current < end) {
      const minutesInHour = Math.min(
        60 - current.minute,
        Math.ceil((end.diff(current).as('minutes')))
      );

      _emit([
        current.toISODate(),
        doc.data.field || 'unknown',
        current.hour
      ], minutesInHour);

      current = current.plus({ hours: 1 }).startOf('hour');
    }
  },
  reduce: "_sum"
};

function parseDuration(duration: isoDuration): number {
  const matches = duration.match(/P(?:(\d+)Y)?(?:(\d+)M)?(?:(\d+)D)?T(?:(\d+)H)?(?:(\d+)M)?(?:(\d+)S)?/);
  if (!matches) return 0;

  const [, years, months, days, hours, minutes, seconds] = matches;

  return (
    (parseInt(years || '0') * 365 * 24 * 60 * 60 * 1000) +
    (parseInt(months || '0') * 30 * 24 * 60 * 60 * 1000) +
    (parseInt(days || '0') * 24 * 60 * 60 * 1000) +
    (parseInt(hours || '0') * 60 * 60 * 1000) +
    (parseInt(minutes || '0') * 60 * 1000) +
    (parseInt(seconds || '0') * 1000)
  );
}