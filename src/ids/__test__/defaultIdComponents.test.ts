import { defaultIdComponents } from "../defaultIdComponents";
import { exampleDataOccur, exampleOccurTime } from "./exampleData";

describe("defaultIdComponents", () => {
  it("can use occurTime", () => {
    expect(defaultIdComponents({ data: exampleDataOccur })).toMatchObject({
      defaultIdParts: ["%occurTime%"],
    });
  });

  it("uses createTime if no occurTime is present", () => {
    expect(
      defaultIdComponents({
        data: { occurTime: { utc: exampleOccurTime } },
        meta: { createTime: { utc: exampleOccurTime } },
      }),
    ).toMatchObject({ defaultIdParts: ["%occurTime%"] });

    expect(
      defaultIdComponents({
        data: { foo: "bar" },
        meta: { createTime: { utc: exampleOccurTime } },
      }),
    ).toMatchObject({ defaultIdParts: ["%?createTime%"] });
  });

  it("uses a concatenation of data fields if there is no occurTime and no createTime", () => {
    const simpleData = { firstKey: "firstData", secondKey: "secondData" };
    expect(defaultIdComponents({ data: simpleData })).toMatchObject({
      defaultIdParts: ["%firstKey%", "%secondKey%"],
    });
  });

  it("uses field as the default partition", () => {
    expect(
      defaultIdComponents({
        data: { field: "abc", occurTime: { utc: exampleOccurTime } },
      }),
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
          occurTime: { utc: exampleOccurTime },
        },
      }),
    ).toMatchObject({ defaultPartitionParts: ["%field%"] });
    expect(
      defaultIdComponents({
        data: { field: "works", with: "other", keys: "too" },
      }),
    ).toMatchObject({ defaultPartitionParts: ["%field%"] });
  });

  it("returns undefined for defaultPartitionParts when no field is present", () => {
    expect(
      defaultIdComponents({
        data: {
          no: "field",
          key: "present",
          occurTime: { utc: exampleOccurTime },
        },
      }),
    ).toMatchObject({ defaultPartitionParts: undefined });
    expect(
      defaultIdComponents({
        data: { no: "field", key: "present" },
      }),
    ).toMatchObject({ defaultPartitionParts: undefined });
  });
});
