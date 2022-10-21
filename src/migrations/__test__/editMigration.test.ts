import { DocumentScope } from "nano";
import { testDbLifecycle } from "../../test-utils";
import { editMigration } from "../editMigration";
import { asViewDb } from "../../views/viewDocument";
import * as editInTerminal from "../../utils/editInTerminal";

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
  const db = testDbLifecycle(dbName);

  it("creates a _design document with the text of mapFn", async () => {
    await editMigration({
      db: db,
      migrationName: "rename_a_to_b",
      mapFn: migA2B,
    });
    await db.view("migrate_rename_a_to_b", "migration").catch(fail);
    const designDoc = await asViewDb(db).get("_design/migrate").catch(fail);
    expect(designDoc.views.rename_a_to_b.map).toBe(migA2B);
  });

  it("can create a second view function without overwriting the first", async () => {
    await editMigration({
      db: db,
      migrationName: "rename_a_to_b",
      mapFn: migA2B,
    });
    await editMigration({
      db: db,
      migrationName: "rename2",
      mapFn: migA2B,
    });
    await db.view("migrate", "rename_a_to_b").catch(fail);
    await db.view("migrate", "rename2").catch(fail);
  });

  it("opens a terminal editor if no mapFn is supplied", async () => {
    const mockedEditInTerminal = jest
      .spyOn(editInTerminal, "editInTerminal")
      .mockImplementation(async () => migA2B);

    await editMigration({
      db: db,
      migrationName: "nonEditedMigration",
      mapFn: migA2B,
    });
    expect(mockedEditInTerminal).not.toHaveBeenCalled();

    await editMigration({ db: db, migrationName: "manuallyEditedMigration" });
    expect(mockedEditInTerminal).toBeCalledTimes(1);
    const designDoc = await db.get("_design/migrate").catch(fail);
    expect(designDoc.views.manuallyEditedMigration.map).toBe(migA2B);
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
    expect(mockedEditInTerminal).toHaveBeenCalledWith(migB2A);

    const designDoc = await asViewDb(db).get("_design/migrate").catch(fail);
    expect(designDoc.views.savedMigration.map).toBe(migB2A);
  });
});