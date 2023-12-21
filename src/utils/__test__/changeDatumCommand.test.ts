import { DatumData } from "../../documentControl/DatumDocument";
import { changeDatumCommand } from "../changeDatumCommand";
import { toDatumTime } from "../../time/timeUtils";
import { setNow } from "../../__test__/test-utils";
import { DataArgs } from "../../input/dataArgs";

const nowStr = "2023-12-20T17:00:00.000Z";
const occurTime = toDatumTime("2023-12-20T17:00:00.000Z");
setNow("2023-12-20T17:00:00.000Z");
describe("changeDatumCommand", () => {
  let datumData: DatumData;
  describe("start", () => {
    it("adds state.id true if state doesn't exist", () => {
      datumData = {
        existing: "data",
      };
      changeDatumCommand(datumData, "start");
      expect(datumData).toMatchObject({
        existing: "data",
        state: {
          id: true,
        },
      });
    });

    it("keeps a preexisting state", () => {
      datumData = {
        state: { id: "outside", weather: "rainy" },
        field: "environment",
      };
      changeDatumCommand(datumData, "start");
      expect(datumData).toMatchObject({
        state: { id: "outside", weather: "rainy" },
        field: "environment",
      });
    });

    it("keeps a prexisting state but changes state.id to true if state.id is false", () => {
      datumData = {
        field: "project",
        state: {
          id: false,
          countdown: 10,
        },
      };
      changeDatumCommand(datumData, "start");
      expect(datumData).toMatchObject({
        field: "project",
        state: {
          id: true,
          countdown: 10,
        },
      });
    });

    it("adds occurTime=now if it doesn't exist", () => {
      datumData = {
        existing: "data",
      };
      changeDatumCommand(datumData, "start");
      expect(datumData).toMatchObject({
        occurTime,
      });
    });

    it("keeps an existing occurTime", () => {
      const explicitOccur = toDatumTime("2023-12-21T11:45:00.000");
      datumData = {
        occurTime: explicitOccur,
      };
      changeDatumCommand(datumData, "start");
      expect(datumData).toMatchObject({
        occurTime: explicitOccur,
      });
    });

    it("removes dur=null from the data", () => {
      datumData = {
        state: { id: "something" },
        dur: null,
      };
      changeDatumCommand(datumData, "start");
      expect(datumData).toMatchObject({
        state: { id: "something" },
      });
      expect(datumData).not.toHaveProperty("dur");
    });

    it("adds a dur key to the beginning of the optional keys array", () => {
      datumData = {
        state: { id: "something" },
      };
      const args: DataArgs = { optional: ["existingKey"] };
      changeDatumCommand(datumData, "start", args);
      expect(args.optional).toEqual(["dur", "existingKey"]);
    });
  });

  describe("end", () => {
    it("adds state.id=false if state doesn't exist", () => {
      datumData = {
        existing: "data",
      };
      changeDatumCommand(datumData, "end");
      expect(datumData).toMatchObject({
        existing: "data",
        state: {
          id: false,
        },
      });
    });

    it("keeps state the same if it is false or { id: false }", () => {
      datumData = {
        state: { id: false },
      };
      changeDatumCommand(datumData, "end");
      expect(datumData).toMatchObject({
        state: { id: false },
      });
      expect(datumData).not.toHaveProperty("lastState");

      datumData = {
        state: false,
      };
      changeDatumCommand(datumData, "end");
      expect(datumData).toMatchObject({
        state: false,
      });
      expect(datumData).not.toHaveProperty("lastState");
    });

    it("turns a preexisting state into lastState and adds a state.id = false", () => {
      datumData = {
        state: { id: "outside", weather: "rainy" },
        field: "environment",
      };
      changeDatumCommand(datumData, "end");
      expect(datumData).toEqual({
        state: { id: false },
        field: "environment",
        lastState: { id: "outside", weather: "rainy" },
        occurTime,
      });
    });

    it("does not override existing lastState", () => {
      datumData = {
        state: { id: "inside" },
        lastState: { id: "outside", weather: "rainy" },
        field: "environment",
      };
      changeDatumCommand(datumData, "end");
      expect(datumData).toEqual({
        lastState: { id: "outside", weather: "rainy" },
        state: { id: false },
        field: "environment",
        occurTime,
      });
    });

    it("adds occurTime=now if it doesn't exist", () => {
      datumData = {
        existing: "data",
      };
      changeDatumCommand(datumData, "end");
      expect(datumData).toMatchObject({
        occurTime,
      });
    });

    it("keeps an existing occurTime", () => {
      const explicitOccur = toDatumTime("2023-12-21T11:45:00.000");
      datumData = {
        occurTime: explicitOccur,
      };
      changeDatumCommand(datumData, "end");
      expect(datumData).toMatchObject({
        occurTime: explicitOccur,
      });
    });

    it("removes dur=null from the data", () => {
      datumData = {
        state: { id: "something" },
        dur: null,
      };
      changeDatumCommand(datumData, "end");
      expect(datumData).toMatchObject({
        state: { id: false },
      });
      expect(datumData).not.toHaveProperty("dur");
    });
    it("adds a dur key to the beginning of the optional keys array", () => {
      datumData = {
        state: { id: "something" },
      };
      const args: DataArgs = { optional: ["existingKey"] };
      changeDatumCommand(datumData, "end", args);
      expect(args.optional).toEqual(["dur", "existingKey"]);
    });
  });

  describe("occur", () => {
    it("adds dur=null to the data", () => {
      datumData = {
        state: { id: "something" },
      };
      changeDatumCommand(datumData, "occur");
      expect(datumData).toMatchObject({
        state: { id: "something" },
        dur: null,
      });
    });
    it("overwrites an existing dur with null", () => {
      datumData = {
        state: { id: "something" },
        dur: "PT10M",
      };
      changeDatumCommand(datumData, "occur");
      expect(datumData).toMatchObject({
        state: { id: "something" },
        dur: null,
      });
    });

    it("adds occurTime=now if it doesn't exist", () => {
      datumData = {
        existing: "data",
      };
      changeDatumCommand(datumData, "occur");
      expect(datumData).toMatchObject({
        occurTime,
      });
    });
    it("keeps an existing occurTime", () => {
      const explicitOccur = toDatumTime("2023-12-21T11:45:00.000");
      datumData = {
        occurTime: explicitOccur,
      };
      changeDatumCommand(datumData, "occur");
      expect(datumData).toMatchObject({
        occurTime: explicitOccur,
      });
    });
  });

  describe("switch", () => {
    it("sets state to be {}", () => {
      datumData = {
        field: "field",
      };
      changeDatumCommand(datumData, "switch");
      expect(datumData).toMatchObject({
        field: "field",
        state: {},
      });
    });

    it("overwrites existing state with {}", () => {
      datumData = {
        state: { id: "something" },
        occurTime,
      };
      changeDatumCommand(datumData, "switch");
      expect(datumData).toEqual({
        state: {},
        occurTime,
      });
    });

    it("adds occurTime=now if it doesn't exist", () => {
      datumData = {
        existing: "data",
      };
      changeDatumCommand(datumData, "switch");
      expect(datumData).toMatchObject({
        occurTime,
      });
    });
    it("keeps an existing occurTime", () => {
      const explicitOccur = toDatumTime("2023-12-21T11:45:00.000");
      datumData = {
        occurTime: explicitOccur,
      };
      changeDatumCommand(datumData, "switch");
      expect(datumData).toMatchObject({
        occurTime: explicitOccur,
      });
    });

    it("removes dur=null from the data", () => {
      datumData = {
        state: { id: "something" },
        dur: null,
      };
      changeDatumCommand(datumData, "switch");
      expect(datumData).toEqual({
        state: {},
        occurTime,
      });
      expect(datumData).not.toHaveProperty("dur");
    });

    it("keeps an existing dur", () => {
      const explicitDur = "PT10M";
      datumData = {
        state: { id: "something" },
        dur: explicitDur,
        occurTime,
      };
      changeDatumCommand(datumData, "switch");
      expect(datumData).toEqual({
        state: {},
        dur: explicitDur,
        occurTime,
      });
    });

    it("adds a state=true and a dur key to the beginning of the optional keys array", () => {
      datumData = {
        state: { id: "something" },
      };
      const args: DataArgs = { optional: ["existingKey"] };
      changeDatumCommand(datumData, "switch", args);
      expect(args.optional).toEqual(["state=true", "dur", "existingKey"]);
    });
  });
});
