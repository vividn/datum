import { interpolateFields } from "../interpolateFields";
import { DatumData, DatumMetadata } from "../../documentControl/DatumDocument";

describe("interpolateFields", () => {
  it("interpolates %fields% and %?metaFields% of datum doc", async () => {
    const data: DatumData = {
      foo: "bar",
      another: 123,
    };
    const meta: DatumMetadata = {
      humanId: "abcde",
    };

    const interpolated = interpolateFields({
      data,
      meta,
      format: "raw_string_%foo%???%?humanId%",
    });
    expect(interpolated).toEqual("raw_string_bar???abcde");
  });
});
