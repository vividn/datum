import { testDbLifecycle } from "../../__test__/test-utils.js";
import * as insertDatumViewModule from "../insertDatumView.js";
import { setupDatumViews } from "../setupDatumViews.js";
import { _emit } from "../emit.js";
import * as getAllDatumViews from "../getAllDatumViews.js";
import { DatumView } from "../DatumView.js";

describe("setupDatumViews", () => {
  const dbName = "setup_datum_views_test";
  const db = testDbLifecycle(dbName);

  function emit(key: any, value: any) {
    _emit(key, value);
  }

  afterEach(async () => {
    jest.resetModules();
  });

  it("adds all datum views and db views to an empty db", async () => {
    const datumView1: DatumView = {
      name: "datum_view",
      map: (doc: any) => {
        emit(doc._id, 1);
      },
      reduce: "_count",
    };
    const datumView2: DatumView = {
      name: "datum_another_view",
      map: (doc: any) => {
        emit(doc._rev, null);
      },
    };

    const mockAllDatumViews = [datumView1, datumView2];
    jest
      .spyOn(getAllDatumViews, "getAllDatumViews")
      .mockReturnValue(mockAllDatumViews);

    const dbView1: DatumView = {
      name: "project_view",
      map: (doc: any) => {
        emit(doc._id, 3);
      },
    };
    const mockDbDatumViews = [dbView1];
    jest
      .spyOn(getAllDatumViews, "getDbDatumViews")
      .mockResolvedValue(mockDbDatumViews);

    const insertDatumViewsSpy = jest.spyOn(
      insertDatumViewModule,
      "insertDatumView",
    );

    await setupDatumViews({ db });

    await db.get("_design/datum_view");
    await db.get("_design/datum_another_view");
    await expect(db.get("_design/project_view")).rejects.toMatchObject({
      name: "not_found",
      reason: "missing",
    });
    expect(insertDatumViewsSpy).toHaveBeenCalledTimes(2);

    await setupDatumViews({ db, projectDir: "./" });
    await db.get("_design/project_view");
  });
});
