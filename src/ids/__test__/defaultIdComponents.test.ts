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
    ).toMatchObject({ defaultIdParts: ["%?createTime%c"] });
  });

  it("uses a concatenation of data fields if there is no occurTime and no createTime", () => {
    const simpleData = { firstKey: "firstData", secondKey: "secondData" };
    expect(defaultIdComponents({ data: simpleData })).toMatchObject({
      defaultIdParts: ["%firstKey%", "%secondKey%"],
    });
  });
});
