import { DatumDocument } from "../../../src/documentControl/DatumDocument.js";
import {
  DatumMigration,
  migrationEmit,
} from "../../../src/migrations/migrations.js";

const emit = migrationEmit;

export const migrateExample: DatumMigration = {
  name: "migrate_example",
  map: (doc) => {
    const { data } = doc as DatumDocument<{ a: string }>;
    if (data && data.a) {
      emit(null, { op: "rekey", data: { a: "newKey" } });
    }
  },
  reduce: "_count",
};
