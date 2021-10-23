import { describe, expect, it } from "@jest/globals";
import { defaultIdComponents } from "../defaultIdComponents";
import { exampleDataOccur, exampleOccurTime } from "./exampleData";

describe("defaultIdComponents", () => {
  it("can use occurTime", () => {
    expect(defaultIdComponents({ data: exampleDataOccur })).toMatchObject({
      defaultIdParts: ["%occurTime%"],
    });
  });

  it("uses a concatenation of data fields if hasOccurTime is false", () => {
    const simpleData = { firstKey: "firstData", secondKey: "secondData" };
    expect(defaultIdComponents({ data: simpleData })).toMatchObject({
      defaultIdParts: ["%firstKey%", "%secondKey%"],
    });
  });

  it("uses field as the default partition", () => {
    expect(
      defaultIdComponents({
        data: { field: "abc", occurTime: exampleOccurTime },
      })
    ).toMatchObject({
      defaultPartitionParts: ["%field%"],
    });
    expect(defaultIdComponents({ data: { field: "abc" } })).toMatchObject({
      defaultPartitionParts: ["%field%"],
    });
    expect(
      defaultIdComponents({
        data: {
          field: "works",
          with: "other",
          keys: "too",
          occurTime: exampleOccurTime,
        },
      })
    ).toMatchObject({ defaultPartitionParts: ["%field%"] });
    expect(
      defaultIdComponents({
        data: { field: "works", with: "other", keys: "too" },
      })
    ).toMatchObject({ defaultPartitionParts: ["%field%"] });
  });

  it("returns undefined for defaultPartitionParts when no field is present", () => {
    expect(
      defaultIdComponents({
        data: { no: "field", key: "present", occurTime: exampleOccurTime },
      })
    ).toMatchObject({ defaultPartitionParts: undefined });
    expect(
      defaultIdComponents({
        data: { no: "field", key: "present" },
      })
    ).toMatchObject({ defaultPartitionParts: undefined });
  });
});
