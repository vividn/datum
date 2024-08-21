import { testDbLifecycle } from "../../__test__/test-utils";
import { insertDatumView } from "../../views/insertDatumView";
import { DatumMigration, migrationEmit } from "../migrations";

const emit = migrationEmit;

const aTestMigration: DatumMigration = {
  name: "migrate_test_run_migration",
  emit,
  map: (doc) => {
    const { data } = doc;
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
