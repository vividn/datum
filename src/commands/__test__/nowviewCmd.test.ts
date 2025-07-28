import {
  mockedLogLifecycle,
  mockSpecs,
  testDbLifecycle,
  setNow,
} from "../../__test__/test-utils";
import { generateSampleMorning } from "../../__test__/generateSampleMorning";
import { setupCmd } from "../setupCmd";
import { nowviewCmd } from "../nowviewCmd";
import fs from "fs";

describe("nowview", () => {
  const dbName = "nowview_test";
  testDbLifecycle(dbName);
  mockedLogLifecycle();
  mockSpecs();

  beforeEach(async () => {
    setupCmd({});
  });

  it("consistently generates a sample svg of past 15 minutes", async () => {
    await generateSampleMorning("2024-11-20");
    setNow("9:59 2024-11-20");
    await nowviewCmd({
      width: 800,
      height: 100,
      outputFile: "/tmp/sample_nowview.svg",
    });
    const svg = fs.readFileSync("/tmp/sample_nowview.svg", "utf8");
    expect(svg).toMatchSnapshot();
    // write svg to snapshot folder too for github viewing
    fs.writeFileSync(__dirname + "/__snapshots__/sample_nowview.svg", svg);
  });
});
