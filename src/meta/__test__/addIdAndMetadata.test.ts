import { addIdAndMetadata } from "../addIdAndMetadata";
import { setNow } from "../../test-utils";
import { IdError } from "../../errors";

describe("addIdAndMetadata", () => {
  beforeEach(() => {
    setNow("2023-09-05,11:35");
  });

  it("adds id and metadata in a sane and stable way 1", () => {
    expect(addIdAndMetadata({ abc: "ghi" }, {})).toEqual({
      _id: "ghi",
      data: {
        abc: "ghi",
      },
      meta: {
        createTime: "2023-09-05T11:35:00.000Z",
        humanId: expect.any(String),
        idStructure: "%abc%",
        modifyTime: "2023-09-05T11:35:00.000Z",
        random: expect.any(Number),
      },
    });
  });

  it("adds id and metadata in a sane and stable way 2", () => {
    expect(
      addIdAndMetadata(
        { abc: "ghi", occurTime: { utc: "2023-09-05T11:20:00.000Z" } },
        {}
      )
    ).toEqual({
      _id: "2023-09-05T11:20:00.000Z",
      data: {
        abc: "ghi",
        occurTime: "2023-09-05T11:20:00.000Z",
      },
      meta: {
        idStructure: "%occurTime%",
        createTime: "2023-09-05T11:35:00.000Z",
        humanId: expect.any(String),
        modifyTime: "2023-09-05T11:35:00.000Z",
        random: expect.any(Number),
      },
    });
  });

  it("adds id and metadata in a sane and stable way 3", () => {
    expect(
      addIdAndMetadata(
        {
          abc: "ghi",
          occurTime: { utc: "2023-09-05T11:20:00.000Z" },
        },
        { noMetadata: true }
      )
    ).toEqual({
      _id: "2023-09-05T11:20:00.000Z",
      abc: "ghi",
      occurTime: "2023-09-05T11:20:00.000Z",
    });
  });

  it("adds id and metadata in a sane and stable way 4", () => {
    expect(
      addIdAndMetadata(
        {
          abc: "ghi",
          occurTime: { utc: "2023-09-05T11:20:00.000Z" },
        },
        { idPart: ["%abc"] }
      )
    ).toEqual({
      _id: "ghi",
      data: {
        abc: "ghi",
        occurTime: "2023-09-05T11:20:00.000Z",
      },
      meta: {
        idStructure: "%abc%",
        createTime: "2023-09-05T11:35:00.000Z",
        humanId: expect.any(String),
        modifyTime: "2023-09-05T11:35:00.000Z",
        random: expect.any(Number),
      },
    });
  });

  it("adds id and metadata in a sane and stable way 5", () => {
    expect(
      addIdAndMetadata(
        {
          foo: "bar",
          field: "field",
          occurTime: { utc: "2023-09-05T11:20:00.000Z" },
        },
        {}
      )
    ).toEqual({
      _id: "field:2023-09-05T11:20:00.000Z",
      data: {
        field: "field",
        foo: "bar",
        occurTime: "2023-09-05T11:20:00.000Z",
      },
      meta: {
        idStructure: "%field%:%occurTime%",
        createTime: "2023-09-05T11:35:00.000Z",
        humanId: expect.any(String),
        modifyTime: "2023-09-05T11:35:00.000Z",
        random: expect.any(Number),
      },
    });
  });

  it("adds id and metadata in a sane and stable way 6", () => {
    const payload = addIdAndMetadata(
      {
        foo: "bar",
        field: "field",
        occurTime: { utc: "2023-09-05T11:20:00.000Z" },
      },
      {
        partition: "%foo",
        idPart: "%?humanId",
      }
    );
    expect(payload).toMatchObject({
      data: {
        field: "field",
        foo: "bar",
        occurTime: "2023-09-05T11:20:00.000Z",
      },
      meta: {
        idStructure: "%foo%:%?humanId%",
      },
    });
    const hid = payload.meta.humanId;
    expect(payload._id).toEqual(`bar:${hid}`);
  });

  it("adds id and metadata in a sane and stable way 7", () => {
    const payload = addIdAndMetadata(
      {
        foo: "bar",
        field: "field",
        occurTime: { utc: "2023-09-05T11:20:00.000Z" },
      },
      {
        partition: "%foo",
        idPart: ["%occurTime", "%?humanId"],
        idDelimiter: "!!!",
      }
    );
    expect(payload).toMatchObject({
      data: {
        field: "field",
        foo: "bar",
        occurTime: "2023-09-05T11:20:00.000Z",
      },
      meta: {
        idStructure: "%foo%:%occurTime%!!!%?humanId%",
      },
    });
    const hid = payload.meta.humanId;
    expect(payload._id).toEqual(`bar:2023-09-05T11:20:00.000Z!!!${hid}`);
  });

  it("throws an error if the derived id is blank", () => {
    expect(() => addIdAndMetadata({}, {})).toThrow(IdError);
    expect(() => addIdAndMetadata({ foo: "bar" }, { idPart: [""] })).toThrow(
      IdError
    );
  });
});
