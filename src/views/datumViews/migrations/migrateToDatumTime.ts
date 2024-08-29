import { DatumDocument, DatumMetadata } from "../../../documentControl/DatumDocument";
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

export const migrateDatumTime2: DatumMigration = {
  name: "migrate_datum_time_2",
  emit,
  map: (doc) => {
    const { meta } = doc as DatumDocument;
    // @ts-expect-error old metdata format
    const { createTime, modifyTime, utcOffset, ...rest } = meta;

    if (
      typeof createTime === "string" ||
      typeof modifyTime === "string" ||
      typeof utcOffset !== "undefined"
    ) {
      const newCreateTime =
        typeof createTime === "string"
          ? { utc: createTime, o: utcOffset }
          : createTime;
      const newModifyTime =
        typeof modifyTime === "string"
          ? { utc: modifyTime, o: utcOffset }
          : modifyTime;
      const newMeta = {
        ...rest,
        createTime: newCreateTime,
        modifyTime: newModifyTime,
        // also handily deletes utcOffset
      } as DatumMetadata;
      emit(3, { data: { ...doc, meta: newMeta }, op: "overwrite" });
    }
  },
  reduce: "_count",
};
