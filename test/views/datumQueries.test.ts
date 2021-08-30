import { afterEach, beforeAll, beforeEach, expect, test } from "@jest/globals";
import { DatumPayload, EitherDocument, EitherPayload } from "../../src/documentControl/DatumDocument";
import { pass, testNano } from "../test-utils";
import { minId } from "../../src/views/datumViews/humanId";
import { DocumentScope } from "nano";

const dbName = "test_datum_queries";
const db = testNano.db.use(dbName) as DocumentScope<EitherPayload>;
beforeAll(async () => {
  await testNano.db.destroy(dbName).catch(pass);
});

beforeEach(async () => {
  await testNano.db.create(dbName);
  // TODO: Get all datum views in
});

afterEach(async () => {
  await testNano.db.destroy(dbName);
});

test.skip("minId returns the smallest non-conflicting humanId", async () => {
  const hid1 = "abcd_then_different";
  const hid2 = "abcd_then_divergent";

  const doc1: DatumPayload = {
    _id: "doc1",
    data: {},
    meta: { humanId: hid1 },
  };
  const doc2: DatumPayload = {
    _id: "doc2",
    data: {},
    meta: { humanId: hid2 },
  };
  await db.insert(doc1);
  await db.insert(doc2);

  expect(await minId(db, hid1)).toBe("abcd_then_dif");
  expect(await minId(db, hid2)).toBe("abcd_then_div");
});
test.todo("minId throws if the full string matches multiple docs");
test.todo("minId throws if no slice matches exactly one document");
test.todo("minId throws if view does not exist");