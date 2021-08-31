import { it, describe, expect } from "@jest/globals";
import {
  DatumView,
  datumViewToViewPayload,
} from "../../src/views/viewDocument";
import emit from "../../src/views/emit";

const genericMapFunction = (doc: any) => {
      emit(doc._id, null);
    };
const genericMapStr = genericMapFunction.toString();
const genericReduceFunction = (_keys: any[], _values: any[], _rereduce: boolean) => {
      return 0;
    };
const genericReduceStr = genericReduceFunction.toString();

describe("datumViewToViewPayload", () => {
  it("turns the name into a _design id", () => {
    const datumView: DatumView = {
      name: "the_name",
      map: (_doc) => {},
    };
    const viewPayload = datumViewToViewPayload(datumView);
    expect(viewPayload).toHaveProperty("_id", "_design/the_name");
  });

  it("has a 'default' view with stringified map doc if no reduce is given", () => {
    const datumView: DatumView = {
      name: "has_a_default_view",
      map: genericMapFunction
    };
    const viewPayload = datumViewToViewPayload(datumView);
    expect(viewPayload).toHaveProperty("views.default");
    expect(viewPayload).toHaveProperty("views.default.map", genericMapStr);
  });

  it(
    "uses a 'default' view with stringified map if just one reduce is given",
    () => {
      const datumView: DatumView = {
        name: "with_reduce_still_has_default",
        map: genericMapFunction,
        reduce: "_count"
      };
      const viewPayload = datumViewToViewPayload(datumView);
      expect(viewPayload).toHaveProperty("views.default");
      expect(viewPayload).toHaveProperty(
        "views.default.map",
        genericMapStr
      );
    }
  );

  it("stringifies reduce if it is function", () => {
    const datumView: DatumView = {
        name: "stringified_reduce",
        map: genericMapFunction,
        reduce: genericReduceFunction
      };
    const viewPayload = datumViewToViewPayload(datumView);
    expect(viewPayload).toHaveProperty("views.default.reduce", genericMapStr);
  });

  it("keeps the reduce string if it is a special case", () => {
    const viewPayload = datumViewToViewPayload({
      name: "special_reduce_string",
      map: genericMapFunction,
      reduce: "_count"
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
        anotherView: genericReduceFunction
      }
    });
    const expectedViews = {
      views: {
        anotherView: {
          map: genericMapStr,
          reduce: genericReduceStr
        },
        count: {
          map: genericMapStr,
          reduce: "_count"
        },
        default: {
          map: genericMapStr,
          reduce: genericReduceStr
        }
      }
    };
    expect(viewPayload).toMatchObject(expectedViews);
    }
  );

  it("has a default view with just the map document if no reduce is named default", () => {
    const viewPayload = datumViewToViewPayload({
      name: "multiple_reduce_no_default",
      map: genericMapFunction,
      reduce: {
        count: "_count",
        anotherView: genericReduceFunction
      }
    });
    expect(viewPayload).toHaveProperty("views.default.map", genericMapStr);
    expect(viewPayload).not.toHaveProperty("views.default.reduce");
    }
  );

  it("adds an empty meta object", () => {
    expect(datumViewToViewPayload({
      name: "no_reduce",
      map: genericMapFunction
    })).toHaveProperty("meta", {});
    expect(datumViewToViewPayload({
      name: "one_reduce",
      map: genericMapFunction,
      reduce: "_count"
    })).toHaveProperty("meta", {});
    expect(datumViewToViewPayload({
      name: "several_reduce",
      map: genericMapFunction,
      reduce: {
        one: genericReduceFunction,
        two: genericReduceFunction
      }
    })).toHaveProperty("meta", {});
  });
});

describe("insertDatumView", () => {
  it.todo("turns a DatumView into a functioning view");
  it.todo("overwrites an existing view with the new function contents");
});

it.todo("adds all datum views to an empty db");
