import { pass, resetTestDb } from "../../test-utils";
import { editMigration } from "../editMigration";
import { asViewDb } from "../../views/DatumView";
import * as editInTerminal from "../../utils/editInTerminal";
import { getMigrationId, getMigrationViewName } from "../migrations";
import { EitherPayload } from "../../documentControl/DatumDocument";

const migA2B = `(doc) => {
  if (doc.a) {
    doc.b = doc.a
    delete doc.a
    emit("overwrite", doc)
  }
}`;

const migB2A = `(doc) => {
  if (doc.a) {
    doc.a = doc.b
    delete doc.b
    emit("overwrite", doc)
  }
}`;

describe("editMigration", () => {
  const dbName = "edit_migration_test";
  let db: PouchDB.Database<EitherPayload>;

  beforeEach(async () => {
    db = await resetTestDb(dbName);
  });

  afterEach(async () => {
    await db.destroy().catch(pass);
  });

  it("creates a _design document with the text of mapFn", async () => {
    const migrationName = "rename_a_to_b";
    const viewName = getMigrationViewName(migrationName);
    const migrationId = getMigrationId(migrationName);
    await editMigration({
      db: db,
      migrationName,
      mapFn: migA2B,
    });
    await db.query(`${viewName}/default`).catch(fail);
    const designDoc = await asViewDb(db).get(migrationId).catch(fail);
    expect(designDoc.views.default.map).toBe(migA2B);
  });

  it("opens a terminal editor if no mapFn is supplied", async () => {
    const mockedEditInTerminal = jest
      .spyOn(editInTerminal, "editInTerminal")
      .mockImplementation(async () => migA2B);

    await editMigration({
      db: db,
      migrationName: "nonEditedMigration",
      mapFn: migB2A,
    });
    expect(mockedEditInTerminal).not.toHaveBeenCalled();

    const manualName = "manuallyEditedMigration";
    await editMigration({ db: db, migrationName: manualName });
    expect(mockedEditInTerminal).toBeCalledTimes(1);
    const designDoc = await asViewDb(db)
      .get(getMigrationId(manualName))
      .catch(fail);
    expect(designDoc.views.default.map).toBe(migA2B);
  });

  it("loads the current migration map for editing if one exists and no mapFn is supplied", async () => {
    const mockedEditInTerminal = jest
      .spyOn(editInTerminal, "editInTerminal")
      .mockImplementation(async () => migA2B);

    await editMigration({
      db: db,
      migrationName: "savedMigration",
      mapFn: migB2A,
    });
    await editMigration({ db: db, migrationName: "savedMigration" });

    expect(mockedEditInTerminal).toBeCalledTimes(1);
    expect(mockedEditInTerminal).toHaveBeenCalledWith(
      expect.stringContaining(migB2A)
    );

    const designDoc = await asViewDb(db)
      .get(getMigrationId("savedMigration"))
      .catch(fail);
    expect(designDoc.views.default.map).toBe(migA2B);
  });
});
