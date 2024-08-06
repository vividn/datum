import { fail, testDbLifecycle } from "../../__test__/test-utils";
import { editMigration } from "../editMigration";
import { asViewDb } from "../../views/DatumView";
import * as editInTerminal from "../../utils/editInTerminal";
import { getMigrationId, migrationName } from "../migrations";

const migA2B = `(doc) => {
  if (doc.a) {
    doc.b = doc.a;
    delete doc.a;
    emit(null, {op: "overwrite", data: doc});
  }
}`;

const migB2A = `(doc) => {
  if (doc.a) {
    doc.a = doc.b;
    delete doc.b;
    emit(null, {op: "overwrite", data: doc});
  }
}`;

describe("editMigration", () => {
  const dbName = "edit_migration_test";
  const db = testDbLifecycle(dbName);

  it("creates a _design document with the text of mapFn", async () => {
    const name = "rename_a_to_b";
    const viewName = migrationName(name);
    const migrationId = getMigrationId(name);
    await editMigration({
      db: db,
      name,
      mapFn: migA2B,
    });
    await db.query(viewName).catch(fail);
    const designDoc = await asViewDb(db).get(migrationId).catch(fail);
    expect(designDoc.views[viewName].map).toBe(migA2B);
  });

  it("opens a terminal editor if no mapFn is supplied", async () => {
    const mockedEditInTerminal = jest
      .spyOn(editInTerminal, "editInTerminal")
      .mockResolvedValue(migA2B);

    await editMigration({
      db: db,
      name: "nonEditedMigration",
      mapFn: migB2A,
    });
    expect(mockedEditInTerminal).not.toHaveBeenCalled();

    const manualName = "manuallyEditedMigration";
    const manualNameView = migrationName(manualName);
    await editMigration({ db: db, name: manualName });
    expect(mockedEditInTerminal).toBeCalledTimes(1);
    const designDoc = await asViewDb(db)
      .get(getMigrationId(manualName))
      .catch(fail);
    expect(designDoc.views[manualNameView].map).toBe(migA2B);
  });

  it("loads the current migration map for editing if one exists and no mapFn is supplied", async () => {
    const name = "savedMigration";
    const viewName = migrationName(name);
    const mockedEditInTerminal = jest
      .spyOn(editInTerminal, "editInTerminal")
      .mockResolvedValue(migA2B);

    await editMigration({
      db: db,
      name,
      mapFn: migB2A,
    });
    await editMigration({ db: db, name });

    expect(mockedEditInTerminal).toBeCalledTimes(1);
    expect(mockedEditInTerminal).toHaveBeenCalledWith(
      expect.stringContaining(migB2A),
    );

    const designDoc = await asViewDb(db)
      .get(getMigrationId("savedMigration"))
      .catch(fail);
    expect(designDoc.views[viewName].map).toBe(migA2B);
  });
});
