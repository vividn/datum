import * as minHumanId from "../minHumanId";
import * as getHumanIds from "../getHumanIds";
import { DocumentScope } from "nano";
import { testDbLifecycle } from "../../test-utils";
import { insertDatumView } from "../../views/insertDatumView";
import { idToHumanView, subHumanIdView } from "../../views/datumViews";
import { mock } from "jest-mock-extended";
import { shortenForHumans } from "../shortenForHumans";

describe("shortenForHumans", () => {
  const mockDb = mock<PouchDB.Database<any>>();

  it("calls getHumanIds with the array of ids", async () => {
    const ids = ["idA", "idB", "idNo", "idC"];
    const getHumanIdsSpy = jest
      .spyOn(getHumanIds, "getHumanIds")
      .mockReturnValue(Promise.resolve(["aa", "bb", undefined, "cc"]));
    const minHumanIdSpy = jest
      .spyOn(minHumanId, "minHumanId")
      .mockReturnValueOnce(Promise.resolve("a"))
      .mockReturnValueOnce(Promise.resolve("b"))
      .mockReturnValueOnce(Promise.resolve("c"));

    const returnVal = await shortenForHumans(mockDb, ids);

    expect(getHumanIdsSpy.mock.calls).toEqual([[mockDb, ids]]);
    expect(minHumanIdSpy.mock.calls).toEqual([
      [mockDb, "aa"],
      [mockDb, "bb"],
      [mockDb, "cc"],
    ]);

    expect(returnVal).toEqual(["a", "b", undefined, "c"]);
  });
});

describe("integration test", () => {
  const dbName = "test_shorten_for_humans";
  const db = testDbLifecycle(dbName);

  beforeEach(async () => {
    await insertDatumView({ db, datumView: idToHumanView });
    await insertDatumView({ db, datumView: subHumanIdView });
  });

  it("returns an array of shortened humanIds, with undefined holes for docs that have no row in the view", async () => {
    await db.put({
      _id: "id_w_human1",
      data: {},
      meta: { humanId: "abc-111" },
    });
    await db.put({
      _id: "id_w_human2",
      data: {},
      meta: { humanId: "abc-222" },
    });
    await db.put({
      _id: "id_no_human",
      data: {},
      meta: {},
    });

    const returnVal = await shortenForHumans(db, [
      "id_w_human2",
      "id_no_human",
      "id_does_not_exist",
      "id_w_human1",
      "id_w_human1",
      "another_non_id",
    ]);

    expect(returnVal).toEqual([
      "abc-2",
      undefined,
      undefined,
      "abc-1",
      "abc-1",
      undefined,
    ]);
  });
});
