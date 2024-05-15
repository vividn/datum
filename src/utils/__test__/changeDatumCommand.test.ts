import { DatumData } from "../../documentControl/DatumDocument";
import { changeDatumCommand } from "../changeDatumCommand";
import { toDatumTime } from "../../time/timeUtils";
import {
  deterministicHumanIds,
  restoreNow,
  setNow,
  testDbLifecycle,
} from "../../__test__/test-utils";
import { DataArgs } from "../../input/dataArgs";
import { setupCmd } from "../../commands/setupCmd";
import { addCmd } from "../../commands/addCmd";
import { occurCmd } from "../../commands/occurCmd";
import { endCmd } from "../../commands/endCmd";
import { startCmd } from "../../commands/startCmd";
import { switchCmd } from "../../commands/switchCmd";

const nowStr = "2023-12-20T17:00:00.000Z";
const occurTime = toDatumTime(nowStr);
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

    it("adds a dur key to the beginning of the keys array", () => {
      datumData = {
        state: { id: "something" },
      };
      const args: DataArgs = { keys: ["existingKey"] };
      changeDatumCommand(datumData, "start", args);
      expect(args.keys).toEqual(["dur=", "existingKey"]);
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

    it("does not turn a simple state: true or state: {id: true} into lastState", () => {
      datumData = {
        state: { id: true },
      };
      changeDatumCommand(datumData, "end");
      expect(datumData).toEqual({
        state: { id: false },
        occurTime,
      });
      expect(datumData).not.toHaveProperty("lastState");

      datumData = {
        state: true,
      };
      changeDatumCommand(datumData, "end");
      expect(datumData).toEqual({
        state: { id: false },
        occurTime,
      });
      expect(datumData).not.toHaveProperty("lastState");
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
    it("adds a dur key to the beginning of the keys array", () => {
      datumData = {
        state: { id: "something" },
      };
      const args: DataArgs = { keys: ["existingKey"] };
      changeDatumCommand(datumData, "end", args);
      expect(args.keys).toEqual(["dur=", "existingKey"]);
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

    it("adds a state=true and a dur key to the beginning of the keys array", () => {
      datumData = {
        state: { id: "something" },
      };
      const args: DataArgs = { keys: ["existingKey"] };
      changeDatumCommand(datumData, "switch", args);
      expect(args.keys).toEqual(["state.id=true", "dur=", "existingKey"]);
    });
  });
});

describe("changing from one command to another", () => {
  testDbLifecycle("change_cmd_test");
  deterministicHumanIds();

  beforeEach(async () => {
    await setupCmd("");
  });

  describe("addCmd", () => {
    beforeEach(async () => {
      setNow("2023-12-21 14:00");
      await setupCmd("");
    });
    afterAll(() => {
      restoreNow();
    });

    it("can become an occur command", async () => {
      expect(
        await addCmd("field -k req1 -k opt1= reqVal optVal occur"),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become a start command", async () => {
      expect(
        await addCmd("field -k req1 -k opt1= reqVal optVal start '30 min'"),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become an end command", async () => {
      expect(
        await addCmd("field -k req1 -k opt1= reqVal optVal end '30 min'"),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become a switch command", async () => {
      expect(
        await addCmd(
          "field -k req1 -k opt1= reqVal optVal switch stateName 5m30s",
        ),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });
  });

  describe("occurCmd", () => {
    beforeEach(async () => {
      setNow("2023-12-21 14:00");
    });
    afterAll(() => {
      restoreNow();
    });

    it("can become a start command", async () => {
      expect(
        await occurCmd("field -k req1 -k opt1= reqVal optVal start 30min"),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become an end command", async () => {
      expect(
        await occurCmd("field -k req1 -k opt1= reqVal optVal end 30min"),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become a switch command", async () => {
      expect(
        await occurCmd(
          "field -k req1 -k opt1= reqVal optVal switch stateName 5m30s",
        ),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("doesn't require a duration to become a start command", async () => {
      expect(
        await occurCmd("field -k req1 -k opt1= reqVal optVal start"),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });
  });
  describe("startCmd", () => {
    beforeEach(async () => {
      setNow("2023-12-21 14:00");
    });
    afterAll(() => {
      restoreNow();
    });

    it("can become an occur command", async () => {
      expect(
        await startCmd("field 30 -k opt1= key=val optVal occur"),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become an end command", async () => {
      expect(
        await startCmd("field -k opt1= 30 key=val optVal end"),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become a switch command", async () => {
      expect(
        await startCmd("field -k opt1= 5m30s key=val optVal switch stateName"),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });
  });
  describe("endCmd", () => {
    beforeEach(async () => {
      setNow("2023-12-21 14:00");
    });
    afterAll(() => {
      restoreNow();
    });

    it("can become an occur command", async () => {
      expect(
        await endCmd("field -k opt1= 30 key=val optVal occur"),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become start command", async () => {
      expect(
        await endCmd("field -k opt1= 30 key=val optVal start"),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become a switch command", async () => {
      expect(
        await endCmd("field -k opt1= 5m30s key=val optVal switch stateName"),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });
  });
  describe("switchCmd", () => {
    beforeEach(async () => {
      setNow("2023-12-21 14:00");
    });
    afterAll(() => {
      restoreNow();
    });

    it("can become an occur command", async () => {
      expect(
        await switchCmd("field -k opt1= someState 30 key=val optVal occur"),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become an end command", async () => {
      expect(
        await switchCmd("field -k opt1= someState 30 key=val optVal end"),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });

    it("can become a start command", async () => {
      expect(
        await switchCmd("field -k opt1= someState 5m30s key=val optVal start"),
      ).toMatchSnapshot({
        _rev: expect.any(String),
      });
    });
  });
});
