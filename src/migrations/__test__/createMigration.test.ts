import { DocumentScope } from "nano";
import { testDbLifecycle } from "../../test-utils";
import { createMigration } from "../createMigration";
import { asViewDb } from "../../views/viewDocument";
import * as editInTerminal from "../../utils/editInTerminal";

const migA2B = `(doc) => {
  if (doc.a) {
    doc.b = doc.a
    delete doc.a
    emit("overwrite", doc)
  }
}`;

describe.skip("createMigration", () => {
  const dbName = "create_migration_test";
  const db = testDbLifecycle(dbName);

  it("creates a _design document with the text of mapFn", async () => {
    await createMigration({
      db: db,
      migrationName: "rename_a_to_b",
      mapFnStr: migA2B,
    });
    await db.view("migrate", "rename_a_to_b").catch(fail);
    const designDoc = await asViewDb(db).get("_design/migrate").catch(fail);
    expect(designDoc.views.rename_a_to_b.map).toBe(migA2B);
  });

  it("can create a second view function without overwriting the first", async () => {
    await createMigration({
      db: db,
      migrationName: "rename_a_to_b",
      mapFnStr: migA2B,
    });
    await createMigration({
      db: db,
      migrationName: "rename2",
      mapFnStr: migA2B,
    });
    await db.view("migrate", "rename_a_to_b").catch(fail);
    await db.view("migrate", "rename2").catch(fail);
  });

  it("opens a terminal editor if no mapFn is supplied", async () => {
    const mockedEditInTerminal = jest
      .spyOn(editInTerminal, "editInTerminal")
      .mockImplementation(async () => migA2B);

    await createMigration({
      db: db,
      migrationName: "nonEditedMigration",
      mapFnStr: migA2B,
    });
    expect(mockedEditInTerminal).not.toHaveBeenCalled();

    await createMigration({ db: db, migrationName: "manuallyEditedMigration" });
    expect(mockedEditInTerminal).toBeCalledTimes(1);
    const designDoc = await db.get("_design/migrate").catch(fail);
    expect(designDoc.views.manuallyEditedMigration.map).toBe(migA2B);
  });

  it("loads the current migration map for editing if one exists and no mapFn is supplied", async () => {
    const mockedEditInTerminal = jest
      .spyOn(editInTerminal, "editInTerminal")
      .mockImplementation(async () => migAddField);

    await createMigration({
      db: db,
      migrationName: "savedMigration",
      mapFnStr: migA2B,
    });
    await createMigration({ db: db, migrationName: "savedMigration" });

    expect(mockedEditInTerminal).toBeCalledTimes(1);
    expect(mockedEditInTerminal).toHaveBeenCalledWith(migA2B);

    const designDoc = await db.get("_design/migrate").catch(fail);
    expect(designDoc.views.savedMigration.map).toBe(migAddField);
  });
});