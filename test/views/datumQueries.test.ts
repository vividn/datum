import {
  afterEach,
  beforeAll,
  beforeEach,
  expect,
  it,
  describe,
} from "@jest/globals";
import {
  DatumPayload,
  EitherPayload,
} from "../../src/documentControl/DatumDocument";
import { pass, testNano } from "../test-utils";
import { DocumentScope } from "nano";
import { minHumanId, MinHumanIdError } from "../../src/ids/minHumanId";
import insertDatumView from "../../src/views/insertDatumView";
import { subHumanIdView } from "../../src/views/datumViews";
import { DatumViewMissingError } from "../../src/errors";

const dbName = "test_datum_queries";
const db = testNano.db.use(dbName) as DocumentScope<EitherPayload>;
beforeAll(async () => {
  await testNano.db.destroy(dbName).catch(pass);
});

beforeEach(async () => {
  await testNano.db.create(dbName);
});

afterEach(async () => {
  await testNano.db.destroy(dbName);
});

describe("minHumanId", () => {
  beforeEach(async () => {
    await insertDatumView({ db, datumView: subHumanIdView });
  });

  it(" returns the smallest non-conflicting humanId", async () => {
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

    expect(await minHumanId(db, hid1)).toBe("abcd_then_dif");
    expect(await minHumanId(db, hid2)).toBe("abcd_then_div");
  });

  it("throws if the full string matches multiple docs", async () => {
    const hid1 = "abcd_then_same";
    const hid2 = "abcd_then_same";
    const hid3 = "initial_substring_but_different";
    const hid4 = "initial_substring-then-different";

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
    const doc3: DatumPayload = {
      _id: "doc3",
      data: {},
      meta: { humanId: hid3 },
    };
    const doc4: DatumPayload = {
      _id: "doc4",
      data: {},
      meta: { humanId: hid4 },
    };
    await db.insert(doc1);
    await db.insert(doc2);
    await db.insert(doc3);
    await db.insert(doc4);

    await expect(() => minHumanId(db, hid1)).rejects.toThrow(MinHumanIdError);
    await expect(() => minHumanId(db, "initial_substring")).rejects.toThrow(
      MinHumanIdError
    );
  });

  it("throws if no slice matches exactly one document", async () => {
    const hid1 = "abcd_then_different";
    const hid2 = "defg_another";

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

    await expect(() => minHumanId(db, "completely_different")).rejects.toThrow(
      MinHumanIdError
    );
  });

  it("throws if sub_human_id view does not exist", async () => {
    const viewDoc = await db.get("_design/datum_sub_human_id");
    await db.destroy("_design/datum_sub_human_id", viewDoc._rev);

    await expect(() => minHumanId(db, "anything")).rejects.toThrow(
      DatumViewMissingError
    );
  });
});
