import { testDbLifecycle } from "../../test-utils";
import * as insertDatumViewModule from "../insertDatumView";
import setupDatumViews from "../setupDatumViews";
import _emit from "../emit";
import * as getAllDatumViews from "../getAllDatumViews";

const db = testDbLifecycle("setup_datum_views_test");

function emit(key: any, value: any) {
  _emit(key, value);
}

afterEach(async () => {
  jest.resetModules();
});

it("adds all datum views to an empty db", async () => {
  const datumView1 = {
    name: "datum_view",
    map: (doc: any) => {
      emit(doc._id, 1);
    },
    reduce: "_count",
  };
  const datumView2 = {
    name: "datum_another_view",
    map: (doc: any) => {
      emit(doc._rev, null);
    },
  };

  const mockAllDatumViews = [datumView1, datumView2];
  jest.spyOn(getAllDatumViews, "default").mockReturnValue(mockAllDatumViews);

  const insertDatumViewsSpy = jest.spyOn(insertDatumViewModule, "default");

  await setupDatumViews({ db });

  await db.get("_design/datum_view");
  await db.get("_design/datum_another_view");

  expect(insertDatumViewsSpy).toHaveBeenCalledTimes(2);
});
