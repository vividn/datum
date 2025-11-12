import { testDbLifecycle } from "../../__test__/test-utils.js";
import { DatumDocument } from "../../documentControl/DatumDocument.js";
import { insertDatumView } from "../../views/insertDatumView.js";
import { DatumMigration, migrationEmit } from "../migrations.js";

const emit = migrationEmit;

const aTestMigration: DatumMigration = {
  name: "migrate_test_run_migration",
  map: (doc) => {
    const { data } = doc as DatumDocument;
    if (data.condition === true) {
      data.condition = false;
      emit(null, { op: "update", data });
    }
  },
  reduce: "_count",
};

describe("runMigration", () => {
  const db = testDbLifecycle("run_migration_test");

  beforeEach(async () => {
    await insertDatumView({
      db,
      datumView: aTestMigration,
    });
  });

  it.todo("migrates all rows in the migration");
  it.todo("batches rows asynchronously based off of key");
});
