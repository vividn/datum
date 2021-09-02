import {
  it,
  test,
  describe,
  expect,
  beforeEach,
  beforeAll,
  afterEach,
} from "@jest/globals";
import {
  DatumView,
  datumViewToViewPayload,
} from "../../src/views/viewDocument";
import _emit from "../../src/views/emit";
import { fail, pass, testNano } from "../test-utils";
import { EitherPayload } from "../../src/documentControl/DatumDocument";
import insertDatumView from "../../src/views/insertDatumView";

function emit(key: unknown, value: unknown) {
  _emit(key, value);
}

const genericMapFunction = (doc: any) => {
  emit(doc._id, null);
};

const genericMapStr = `doc => {
  emit(doc._id, null);
}`;

const genericReduceFunction = (
  _keys: any[],
  _values: any[],
  _rereduce: boolean
) => {
  return 0;
};
const genericReduceStr = `(_keys, _values, _rereduce) => {
  return 0;
}`;

test("string representations match toString return value", () => {
  expect(genericMapFunction.toString()).toEqual(genericMapStr);
  expect(genericReduceFunction.toString()).toEqual(genericReduceStr);
});

describe("datumViewToViewPayload", () => {
  it("turns the name into a _design id", () => {
    const datumView: DatumView = {
      name: "the_name",
      map: (_doc) => {
        return;
      },
    };
    const viewPayload = datumViewToViewPayload(datumView);
    expect(viewPayload).toHaveProperty("_id", "_design/the_name");
  });

  it("has a 'default' view with stringified map doc if no reduce is given", () => {
    const datumView: DatumView = {
      name: "has_a_default_view",
      map: genericMapFunction,
    };
    const viewPayload = datumViewToViewPayload(datumView);
    expect(viewPayload).toHaveProperty("views.default");
    expect(viewPayload).toHaveProperty("views.default.map", genericMapStr);
  });

  it("uses a 'default' view with stringified map if just one reduce is given", () => {
    const datumView: DatumView = {
      name: "with_reduce_still_has_default",
      map: genericMapFunction,
      reduce: "_count",
    };
    const viewPayload = datumViewToViewPayload(datumView);
    expect(viewPayload).toHaveProperty("views.default");
    expect(viewPayload).toHaveProperty("views.default.map", genericMapStr);
  });

  it("stringifies reduce if it is function", () => {
    const datumView: DatumView = {
      name: "stringified_reduce",
      map: genericMapFunction,
      reduce: genericReduceFunction,
    };
    const viewPayload = datumViewToViewPayload(datumView);
    expect(viewPayload).toHaveProperty(
      "views.default.reduce",
      genericReduceStr
    );
  });

  it("keeps the reduce string if it is a special case", () => {
    const viewPayload = datumViewToViewPayload({
      name: "special_reduce_string",
      map: genericMapFunction,
      reduce: "_count",
    });
    expect(viewPayload).toHaveProperty("views.default.reduce", "_count");
  });

  it("uses the names of multiple reduce functions as the views in the design document, all with the same map", () => {
    const viewPayload = datumViewToViewPayload({
      name: "multiple_reduce",
      map: genericMapFunction,
      reduce: {
        count: "_count",
        default: genericReduceFunction,
        anotherView: genericReduceFunction,
      },
    });
    console.log(viewPayload);
    const expectedViews = {
      views: {
        anotherView: {
          map: genericMapStr,
          reduce: genericReduceStr,
        },
        count: {
          map: genericMapStr,
          reduce: "_count",
        },
        default: {
          map: genericMapStr,
          reduce: genericReduceStr,
        },
      },
    };
    expect(viewPayload).toMatchObject(expectedViews);
  });

  it("has a default view with just the map document if no reduce is named default", () => {
    const viewPayload = datumViewToViewPayload({
      name: "multiple_reduce_no_default",
      map: genericMapFunction,
      reduce: {
        count: "_count",
        anotherView: genericReduceFunction,
      },
    });
    expect(viewPayload).toHaveProperty("views.default.map", genericMapStr);
    expect(viewPayload).not.toHaveProperty("views.default.reduce");
  });

  it("adds an empty meta object", () => {
    expect(
      datumViewToViewPayload({
        name: "no_reduce",
        map: genericMapFunction,
      })
    ).toHaveProperty("meta", {});
    expect(
      datumViewToViewPayload({
        name: "one_reduce",
        map: genericMapFunction,
        reduce: "_count",
      })
    ).toHaveProperty("meta", {});
    expect(
      datumViewToViewPayload({
        name: "several_reduce",
        map: genericMapFunction,
        reduce: {
          one: genericReduceFunction,
          two: genericReduceFunction,
        },
      })
    ).toHaveProperty("meta", {});
  });

  it("can also accept prestringified versions of map and reduce", () => {
    const datumView = datumViewToViewPayload({
      name: "stringy_input",
      map: genericMapStr,
      reduce: {
        one: "_sum",
        two: genericReduceStr,
      },
    });
    console.log(datumView);
    expect(datumView).toMatchObject({
      _id: "_design/stringy_input",
      views: {
        one: { map: genericMapStr, reduce: "_sum" },
        two: { map: genericMapStr, reduce: genericReduceStr },
        default: { map: genericMapStr },
      },
      meta: {},
    });
  });
});

describe("insertDatumView", () => {
  const dbName = "insert_datum_view_test";
  const db = testNano.use<EitherPayload>(dbName);
  
  beforeAll(async () => {
    await testNano.db.destroy(dbName).catch(pass);
  });
  beforeEach(async () => {
    await testNano.db.create(dbName);
  });
  afterEach(async () => {
    await testNano.db.destroy(dbName);
  });

  it("turns a DatumView into a functioning view", async () => {
    const summerAB: DatumView = {
      name: "summer",
      map: (doc: any) => {
        if (doc.a) {
          emit("a", doc.a);
        }
        if (doc.b) {
          emit("b", doc.b);
        }
      },
      reduce: "_sum"
    };

    await db.insert({_id: "doc1", a: 3, b:4});
    await db.insert({_id: "doc2", a: 6});

    await insertDatumView(summerAB);

    const total = await db.view("summer", "default");
    expect(total.rows[0].value).toBe(13);

    const grouped = await db.view("summer", "default", {group: true});
    expect(grouped.rows).toEqual([{ key: "a", value: 9}, { key: "b", value: 4}]);

    const unreduced = await db.view("summer", "default", {reduce: false});
    expect(unreduced.total_rows).toEqual(3);
  });
  
  it.todo("overwrites an existing view if DatumView is different contents");
  it.todo("does not overwrite if view is identical")
});

it.todo("adds all datum views to an empty db");
