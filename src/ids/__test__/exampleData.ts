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
export const exampleDataOccur = { occurTime: exampleOccurTime, ...exampleData };
export const exampleDataOccurField: DatumData = { ...exampleDataOccur, field: "main" };
export const exampleMeta: DatumMetadata = {
  createTime: "2020-11-09T00:40:12.544Z",
  modifyTime: "2020-11-09T00:40:12.544Z",
  utcOffset: 1,
  random: 0.7368733800261729,
  humanId: "mqp4znq4cvp3qnj74fgi9",
};