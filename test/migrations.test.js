const utils = require("../src/utils");
const { createMigration, runMigration } = require("../src/migrations");
const nano = require("nano")("http://admin:password@localhost:5983");
const pass = () => {};
const fail = (e) => {
  if (e) {
    throw e;
  } else {
    throw Error;
  }
};

const migA2B = `(doc) => {
  if (doc.a) {
    doc.b = doc.a
    delete doc.a
    emit("overwrite", doc)
  }
}`;
const migAddField = `(doc) => {
  if (!doc.field) {
    doc.field = "field"
    emit("overwrite", doc)
  }
}`;

const docA = {
  _id: "docA",
  a: "someData",
};
const docC = {
  _id: "docC",
  c: "cData",
};
const docAA2B = {
  _id: "docA",
  b: "someData",
};
const docAWithField = {
  _id: "docA",
  a: "someData",
  field: "field",
};

describe("createMigration", () => {
  let db;
  beforeAll(async () => {
    await nano.db.destroy("test_create_migration").catch(pass);
  });

  beforeEach(async () => {
    jest.clearAllMocks();
    await nano.db.create("test_create_migration").catch(fail);
    db = nano.use("test_create_migration");
  });

  afterEach(async () => {
    jest.clearAllMocks();
    await nano.db.destroy("test_create_migration").catch(pass);
  });

  it("creates a _design document with the text of mapFn", async () => {
    await createMigration({
      db: db,
      migrationName: "rename_a_to_b",
      mapFnStr: migA2B,
    });
    await db.view("migrate", "rename_a_to_b").catch(fail);
    const designDoc = await db.get("_design/migrate").catch(fail);
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
      .spyOn(utils, "editInTerminal")
      .mockImplementation(() => migA2B);

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
      .spyOn(utils, "editInTerminal")
      .mockImplementation(() => migAddField);

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

describe("runMigration", () => {
  let db;
  beforeAll(async () => {
    await nano.db.destroy("test_run_migration").catch(pass);
  });

  beforeEach(async () => {
    jest.clearAllMocks();
    await nano.db.create("test_run_migration").catch(fail);
    db = nano.use("test_run_migration");
  });

  afterEach(async () => {
    jest.clearAllMocks();
    await nano.db.destroy("test_run_migration").catch(pass);
  });

  it("can overwrite documents", async () => {
    await createMigration({ db: db, migrationName: "A2B", mapFnStr: migA2B });
    await db.insert(docA);
    await db.insert(docC);

    const viewBefore = await db.view("migrate", "A2B");
    expect(viewBefore.total_rows).toBe(1);

    await runMigration({ db: db, migrationName: "A2B" });
    expect({a: 1, b: "abc"}).toMatchObject({b: "abc", c: undefined})
    const viewAfter = await db.view("migrate", "A2B");
    const newDoc = await db.get("docA");
    expect(newDoc).toEqual(docAA2B);
    expect(viewAfter.total_rows).toBe(0);
  });
});
