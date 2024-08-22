import { DatumDocument } from "../../../documentControl/DatumDocument";
import { DatumMigration, migrationEmit } from "../../../migrations/migrations";
import { DatumTime } from "../../../time/datumTime";

const emit = migrationEmit;

export const migrateDatumTime1: DatumMigration = {
  name: "migrate_datum_time_1",
  emit,
  map: (doc) => {
    const { data, meta } = doc as DatumDocument;
    if (!data) {
      return;
    }

    const { occurTime, occurUtcOffset, ...rest } = data;
    // @ts-expect-error old metdata format
    const { utcOffset } = meta;

    const offset = occurUtcOffset ?? utcOffset;
    if (typeof occurTime === "string" && typeof offset === "number") {
      const datumTime: DatumTime = {
        utc: occurTime,
        o: offset,
      };
      const newData = { ...rest, occurTime: datumTime };

      emit(2, { data: newData, op: "useNew" });
    }
  },
  reduce: "_count",
};
