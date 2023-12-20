import { DatumData } from "../../documentControl/DatumDocument";
import { changeDatumCommand } from "../changeDatumCommand";
import { DataArgs } from "../../input/dataArgs";
import { toDatumTime } from "../../time/timeUtils";

const occurTime = toDatumTime("2023-12-20T17:00:00.000Z");
const noOccurData: DatumData = {
  existing: "data"
};
const occurData: DatumData = {
  existing: "data",
  occurTime
};
const startData: DatumData = {
  existing: "data",
  state: true,
  occurTime
};
const endData: DatumData = {
  existing: "data",
  state: false,
  occurTime,
};

describe("changeDatumCommand", () => {
  let datumData: DatumData;
  describe("start", () => {
    it("adds state.id true if state doesn't exist", () => {
      datumData = {
        existing: "data",
      };
      changeDatumCommand(datumData, "start");
      expect(datumData).toEqual({
        existing: "data",
        state: {
          id: true,
        },
      });
    });

    it.todo("keeps a preexisting state");
    it.todo("adds occurTime=now if it doesn't exist");
    it.todo("removes dur=null from the data");
    it.todo("adds a dur key to the beginning of the optional keys array");
  });

  describe("end", () => {
    it.todo("adds state.id=false if state doesn't exist");
    it.todo(
      "turns a preexisting state into lastState and adds a state.id = false",
    );
    it.todo("adds occurTime=now if it doesn't exist");
    it.todo("removes dur=null from the data");
    it.todo("adds a dur key to the beginning of the optional keys array");
  });

  describe("occur", () => {
    it.todo("adds occurTime=now if it doesn't exist");
    it.todo("adds dur=null to the data");
  });

  describe("switch", () => {
    it.todo("adds occurTime=now if it doesn't exist");
    it.todo("removes state if it exists");
    it.todo("removes dur=null from the data");
    it.todo(
      "adds a state.id=true and a dur key to the beginning of the optional keys array",
    );
  });
});
