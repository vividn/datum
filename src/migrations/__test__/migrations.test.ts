import { fail } from "../../test-utils";

test.skip("Rewrite migrations tests", () => {
  fail();
});
import { DocumentScope } from "nano";

import { createMigration, runMigration } from "../src/migrations";
import { fail, pass, testNano } from "./test-utils";
import { GenericObject } from "../src/GenericObject";

const nano = testNano;

const migA2B = `(doc) => {
  if (doc.a) {
    newDoc = JSON.parse(JSON.stringify(doc))
    newDoc.b = newDoc.a
    delete newDoc.a
    emit("overwrite", newDoc)
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
// const docAWithField = {
//   _id: "docA",
//   a: "someData",
//   field: "field",
// };

describe.skip("runMigration", () => {
  let db: DocumentScope<GenericObject>;
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

  it.skip("can overwrite documents", async () => {
    await createMigration({ db: db, migrationName: "A2B", mapFnStr: migA2B });
    await db.insert(docA);
    await db.insert(docC);

    const viewBefore = await db.view("migrate", "A2B");
    expect(viewBefore.total_rows).toBe(1);

    await runMigration({ db: db, migrationName: "A2B" });
    const viewAfter = await db.view("migrate", "A2B");
    const newDoc = await db.get("docA");
    expect(newDoc).toMatchObject(docAA2B);
    expect(newDoc.a).toBeUndefined();
    expect(viewAfter.total_rows).toBe(0);
  });

  it.skip(
    "uses the _id of the emitting doc if none is in the provided doc",
    fail
  );
});
