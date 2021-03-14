const utils = require("../src/utils");
const { createMigration, runMigration } = require("../src/migrations");
const nano = require("nano")("http://admin:password@localhost:5983");
const pass = () => {};
const fail = () => {
  throw Error;
};

const migrationRenameA2B = `(doc) => {
  if (doc.a) {
    doc.b = doc.a
    delete doc.a
    emit("replace", doc)
  }
}`;
const migAddField = `(doc) => {
  if (!doc.field) {
    doc.field = "field"
    emit("replace", doc)
  }
}`

let db;
describe("createMigration", () => {
  beforeAll(async () => {
    await nano.db.destroy("test_migrations").catch(pass);
  });

  beforeEach(async () => {
    jest.clearAllMocks();
    await nano.db.create("test_migrations").catch(fail);
    db = nano.use("test_migrations");
  });

  afterEach(async () => {
    jest.clearAllMocks();
    await nano.db.destroy("test_migrations").catch(pass)
  })

  it("creates a _design document with the text of mapFn", async () => {
    await createMigration({
      db: db,
      migrationName: "rename_a_to_b",
      mapFnStr: migrationRenameA2B,
    });
    await db.view("migrate", "rename_a_to_b").catch(fail);
    const designDoc = await db.get("_design/migrate").catch(fail);
    expect(designDoc.views.rename_a_to_b.map).toBe(migrationRenameA2B);
  });

  it("can create a second view function without overwriting the first", async () => {
    await createMigration({
      db: db,
      migrationName: "rename_a_to_b",
      mapFnStr: migrationRenameA2B,
    });
    await createMigration({db: db, migrationName: "rename2", mapFnStr: migrationRenameA2B })
    await db.view("migrate", "rename_a_to_b").catch(fail);
    await db.view("migrate", "rename2").catch(fail);
  });

  it("opens a terminal editor if no mapFn is supplied", async () => {
    const mockedEditInTerminal = jest.spyOn(utils, "editInTerminal").mockImplementation(() => migrationRenameA2B)

    await createMigration({db: db, migrationName: "nonEditedMigration", mapFnStr: migrationRenameA2B})
    expect(mockedEditInTerminal).not.toHaveBeenCalled()

    await createMigration({db: db, migrationName: "manuallyEditedMigration"})
    expect(mockedEditInTerminal).toBeCalledTimes(1)
    const designDoc = await db.get("_design/migrate").catch(fail);
    expect(designDoc.views.manuallyEditedMigration.map).toBe(migrationRenameA2B);
  })

  it("loads the current migration map for editing if one exists and no mapFn is supplied", async () => {
    const mockedEditInTerminal = jest.spyOn(utils, "editInTerminal").mockImplementation(() => migAddField)

    await createMigration({db: db, migrationName: "savedMigration", mapFnStr: migrationRenameA2B})
    await createMigration({db: db, migrationName: "savedMigration"})
    
    expect(mockedEditInTerminal).toBeCalledTimes(1);
    expect(mockedEditInTerminal).toHaveBeenCalledWith(migrationRenameA2B)
    
    const designDoc = await db.get("_design/migrate").catch(fail);
    expect(designDoc.views.savedMigration.map).toBe(migAddField)
  })
});
