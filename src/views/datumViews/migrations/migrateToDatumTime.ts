import { DatumDocument } from "../../../documentControl/DatumDocument";
import { DatumMigration, migrationEmit } from "../../../migrations/migrations";
import { DatumTime } from "../../../time/datumTime";

const emit = migrationEmit;

export const migrateDatumTime1: DatumMigration = {
  name: "migrate_datum_time_1",
  emit,
  map: (doc) => {
    const { data } = doc as DatumDocument;
    if (!data) {
      return;
    }

    const { occurTime, occurUtcOffset } = data;
    if (typeof occurTime === "string" && typeof occurUtcOffset === "number") {
      const datumTime: DatumTime = {
        utc: occurTime,
        o: occurUtcOffset,
      };
      data.occurTime = datumTime;
      delete data.occurUtcOffset;

      emit(1, { data: data, op: "useNew" });
    }
  },
  reduce: "_count",
};
