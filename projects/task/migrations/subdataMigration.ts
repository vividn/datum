import {
  DatumMigration,
  migrationEmit,
} from "../../../src/migrations/migrations";

const emit = migrationEmit;
export const subdataMigration: DatumMigration = {
  name: "migrate_subdata",
  map: (doc) => {
    if (doc.meta && !doc.data) {
      const { _id, _rev, meta, ...rest } = doc;
      const newDoc = {
        _id,
        _rev,
        data: rest,
        meta,
      };
      emit(null, { op: "overwrite", data: newDoc });
    }
  },
  reduce: "_count",
};
