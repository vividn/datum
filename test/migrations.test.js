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
    emit(1, doc)
  }
}`;

let db;
describe("createMigration", () => {
  beforeAll(async () => {
    await nano.db.destroy("test_migrations").catch(pass);
  });

  beforeEach(async () => {
    await nano.db.create("test_migrations").catch(fail);
    db = nano.use("test_migrations");
  });

  afterEach(async () => {
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
});
