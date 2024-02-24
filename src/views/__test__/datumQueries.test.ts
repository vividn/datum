import { DatumPayload } from "../../documentControl/DatumDocument";
import { testDbLifecycle } from "../../__test__/test-utils";
import { minHumanId, MinHumanIdError } from "../../ids/minHumanId";
import { insertDatumView } from "../insertDatumView";
import { subHumanIdView } from "../datumViews";
import { DatumViewMissingError } from "../../errors";

jest.retryTimes(3);
describe("minHumanId", () => {
  const dbName = "test_datum_queries";
  const db = testDbLifecycle(dbName);

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
    await db.put(doc1);
    await db.put(doc2);

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
    await db.put(doc1);
    await db.put(doc2);
    await db.put(doc3);
    await db.put(doc4);

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
    await db.put(doc1);
    await db.put(doc2);

    await expect(() => minHumanId(db, "completely_different")).rejects.toThrow(
      MinHumanIdError
    );
  });

  it("throws if sub_human_id view does not exist", async () => {
    const viewDoc = await db.get("_design/datum_sub_human_id");
    await db.remove("_design/datum_sub_human_id", viewDoc._rev);

    await expect(() => minHumanId(db, "anything")).rejects.toThrow(
      DatumViewMissingError
    );
  });
});
