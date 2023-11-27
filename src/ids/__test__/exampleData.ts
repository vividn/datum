import { DatumData, DatumMetadata } from "../../documentControl/DatumDocument";

export const exampleData: DatumData = {
  foo: "abc",
  bar: "def",
  complex: { data: "nested" },
  array: ["various", 2, "data"],
  num: 3,
  "wei%rd": "da%ta",
};
export const exampleOccurTime = "2020-11-09T00:35:10.000Z";
export const exampleDataOccur = {
  occurTime: {
    utc: exampleOccurTime,
    o: 1,
    tz: "Europe/Berlin",
  },
  ...exampleData,
};
export const exampleDataOccurField: DatumData = {
  ...exampleDataOccur,
  field: "main",
};
export const exampleMeta: DatumMetadata = {
  createTime: {
    utc: "2020-11-09T00:40:12.544Z",
    o: 1,
    tz: "Europe/Berlin",
  },
  modifyTime: {
    utc: "2020-11-09T00:40:12.544Z",
    o: 1,
    tz: "Europe/Berlin",
  },
  random: 0.7368733800261729,
  humanId: "mqp4znq4cvp3qnj74fgi9",
};
