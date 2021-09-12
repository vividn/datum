import {
  beforeEach,
  expect,
  it,
  jest,
  describe,
  beforeAll,
  afterAll,
} from "@jest/globals";
import * as minHumanId from "../../src/ids/minHumanId";
import * as getHumanIds from "../../src/ids/getHumanIds";
import { DocumentScope } from "nano";
import { EitherPayload } from "../../src/documentControl/DatumDocument";
import { pass, testNano } from "../test-utils";
import insertDatumView from "../../src/views/insertDatumView";
import { idToHumanView, subHumanIdView } from "../../src/views/datumViews";
import { mock } from "jest-mock-extended";
import shortenForHumans from "../../src/ids/shortenForHumans";

describe("shortenForHumans", () => {
  const mockDb = mock<DocumentScope<any>>();

  beforeEach(() => {
    jest.resetAllMocks();
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  it("calls getHumanIds with the array of ids", async () => {
    const ids = ["idA", "idB", "idNo", "idC"];
    const getHumanIdsSpy = jest
      .spyOn(getHumanIds, "default")
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
  const db: DocumentScope<EitherPayload> = testNano.use(dbName);

  beforeAll(async () => {
    await testNano.db.destroy(dbName).catch(pass);
    await testNano.db.create(dbName);

    await insertDatumView({ db, datumView: idToHumanView });
    await insertDatumView({ db, datumView: subHumanIdView });
  });

  afterAll(async () => {
    await testNano.db.destroy(dbName);
  });

  it("returns an array of shortened humanIds, with undefined holes for docs that have no row in the view", async () => {
    await db.insert({
      _id: "id_w_human1",
      data: {},
      meta: { humanId: "abc-111" },
    });
    await db.insert({
      _id: "id_w_human2",
      data: {},
      meta: { humanId: "abc-222" },
    });
    await db.insert({
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
