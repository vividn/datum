import {
  DatumMigration,
  migrationEmit,
} from "../../../src/migrations/migrations";

const emit = migrationEmit;

export const migrateExample: DatumMigration = {
  name: "migrate_example",
  emit,
  map: (doc) => {
    const { data } = doc;
    if (data.a) {
      emit(null, { op: "rekey", data: { a: "newKey" } });
    }
  },
  reduce: "_count",
};
